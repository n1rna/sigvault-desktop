//! Electrum sync runner for local wallets.
//!
//! Drives `wallet_runtime::ElectrumClient` against the wallet's
//! configured Electrs endpoint, applies the resulting update, and
//! persists. Long-running blocking calls (TCP connect + full_scan) run
//! through `tokio::task::spawn_blocking` / `block_in_place` so the
//! Tauri runtime stays responsive.
//!
//! Per-wallet locking: this function expects the caller has already
//! looked up the `SharedHandle` via `LocalWalletState::get` and
//! acquired exclusive access by passing it in. The handle's per-wallet
//! Mutex is held for the duration of the apply + persist phases — long
//! enough that other ops on the same wallet (send, peek, balance)
//! serialise cleanly, but short enough that other wallets are
//! unaffected.

use std::sync::Arc;

use bdk_wallet::bitcoin::Network;
use serde::Serialize;
use thiserror::Error;
use wallet_runtime::ElectrumClient;

use super::state::SharedHandle;
use super::storage::WalletId;

const FULL_SCAN_STOP_GAP: usize = 20;
const FULL_SCAN_BATCH_SIZE: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct SyncSummary {
    pub wallet_id: String,
    pub tip_height: u32,
    pub txs_synced: usize,
    pub balance_sat: u64,
}

/// Phase the UI surfaces to the user. Values are emitted as
/// snake_case strings so the frontend can match on them.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Connecting,
    FetchingHistory,
    Persisting,
    Complete,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncProgress {
    pub wallet_id: String,
    pub phase: SyncPhase,
    /// 0..=100. Coarse-grained — bdk_electrum's full_scan does not expose
    /// a progress callback, so we only emit boundaries between phases.
    pub percent: u32,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("electrs connect failed for {url}: {message}")]
    Connect { url: String, message: String },
    #[error("electrum full_scan failed: {0}")]
    Scan(String),
    #[error("apply_update failed: {0}")]
    Apply(String),
    #[error("persist failed: {0}")]
    Persist(String),
    #[error("internal: {0}")]
    Internal(String),
}

/// Reports progress to whoever cares — the Tauri command wraps this in
/// an `app_event` emit; tests can pass a no-op.
pub trait ProgressSink: Send + Sync + 'static {
    fn emit(&self, progress: SyncProgress);
}

/// Concrete `ProgressSink` that fans out to a closure. Lets the Tauri
/// command pass a window-emit closure without us depending on tauri
/// types here.
pub struct ClosureSink<F>(pub F)
where
    F: Fn(SyncProgress) + Send + Sync + 'static;

impl<F> ProgressSink for ClosureSink<F>
where
    F: Fn(SyncProgress) + Send + Sync + 'static,
{
    fn emit(&self, progress: SyncProgress) {
        (self.0)(progress)
    }
}

/// No-op sink for tests.
pub struct NoopSink;
impl ProgressSink for NoopSink {
    fn emit(&self, _: SyncProgress) {}
}

fn emit(sink: &Arc<dyn ProgressSink>, wallet_id: &WalletId, phase: SyncPhase, percent: u32, message: impl Into<String>) {
    sink.emit(SyncProgress {
        wallet_id: wallet_id.to_string(),
        phase,
        percent,
        message: message.into(),
    });
}

/// Run a full Electrum scan against the wallet, apply the resulting
/// update, persist, and return a summary the dashboard can render.
///
/// The caller passes a `SharedHandle` that they've looked up from
/// `LocalWalletState::get`. The per-wallet Mutex is acquired here for
/// the whole apply + persist phases.
pub async fn run_sync(
    handle: SharedHandle,
    network: Network,
    electrs_url: String,
    wallet_id: WalletId,
    sink: Arc<dyn ProgressSink>,
) -> Result<SyncSummary, SyncError> {
    let _ = network; // checked at unlock time + by descriptor; reserved for future per-network sync params.

    // Phase 1: connect (blocking I/O on the dedicated blocking pool).
    emit(&sink, &wallet_id, SyncPhase::Connecting, 5, format!("Connecting to {electrs_url}…"));
    let url_for_blocking = electrs_url.clone();
    let client = tokio::task::spawn_blocking(move || ElectrumClient::connect(&url_for_blocking))
        .await
        .map_err(|e| SyncError::Internal(format!("spawn_blocking join: {e}")))?
        .map_err(|e| SyncError::Connect {
            url: electrs_url.clone(),
            message: e.to_string(),
        })?;

    // Phase 2 + 3: scan + apply + persist. All three need the per-wallet
    // handle, so we lock it once and run the blocking work via
    // block_in_place to avoid moving the BDK wallet across tasks.
    emit(
        &sink,
        &wallet_id,
        SyncPhase::FetchingHistory,
        30,
        "Fetching wallet history…",
    );

    let mut handle_guard = handle.lock().await;
    let summary = tokio::task::block_in_place(|| {
        let update = client
            .full_scan(
                &handle_guard.wallet,
                FULL_SCAN_STOP_GAP,
                FULL_SCAN_BATCH_SIZE,
                /* fetch_prev_txouts */ true,
            )
            .map_err(|e| SyncError::Scan(e.to_string()))?;

        // Borrow-split so apply_update + persist can both take &mut.
        let super::state::UnlockedHandle {
            wallet, persister, ..
        } = &mut *handle_guard;

        wallet
            .apply_update(update)
            .map_err(|e| SyncError::Apply(e.to_string()))?;

        wallet
            .persist(persister)
            .map_err(|e| SyncError::Persist(e.to_string()))?;

        let balance = wallet.balance();
        let tip = wallet.latest_checkpoint().height();
        let txs = wallet.transactions().count();
        Ok::<_, SyncError>(SyncSummary {
            wallet_id: wallet_id.to_string(),
            tip_height: tip,
            txs_synced: txs,
            balance_sat: balance.total().to_sat(),
        })
    });

    emit(
        &sink,
        &wallet_id,
        SyncPhase::Persisting,
        90,
        "Saving wallet state…",
    );

    let summary = summary?;

    emit(
        &sink,
        &wallet_id,
        SyncPhase::Complete,
        100,
        format!(
            "Synced — {} txs, balance {} sat",
            summary.txs_synced, summary.balance_sat
        ),
    );

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_phase_serialises_as_snake_case() {
        assert_eq!(serde_json::to_string(&SyncPhase::Connecting).unwrap(), "\"connecting\"");
        assert_eq!(
            serde_json::to_string(&SyncPhase::FetchingHistory).unwrap(),
            "\"fetching_history\""
        );
        assert_eq!(
            serde_json::to_string(&SyncPhase::Persisting).unwrap(),
            "\"persisting\""
        );
        assert_eq!(serde_json::to_string(&SyncPhase::Complete).unwrap(), "\"complete\"");
    }

    #[test]
    fn closure_sink_forwards_progress() {
        use std::sync::Mutex;
        let captured: Arc<Mutex<Vec<SyncProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = captured.clone();
        let sink = ClosureSink(move |p: SyncProgress| {
            cap.lock().unwrap().push(p);
        });
        sink.emit(SyncProgress {
            wallet_id: "test".to_string(),
            phase: SyncPhase::Connecting,
            percent: 5,
            message: "hi".to_string(),
        });
        let v = captured.lock().unwrap();
        assert_eq!(v.len(), 1);
        assert!(matches!(v[0].phase, SyncPhase::Connecting));
    }

    #[test]
    fn noop_sink_does_not_panic() {
        let sink = NoopSink;
        sink.emit(SyncProgress {
            wallet_id: "x".to_string(),
            phase: SyncPhase::Complete,
            percent: 100,
            message: String::new(),
        });
    }
}
