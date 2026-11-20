//! `hooks/useLocalWalletSync.ts` — per-wallet view of the backend
//! `local_wallet_sync_progress` channel.
//!
//! The channel is shared across all wallets; [`watch_sync`] filters the
//! store's [`StoreEvent::SyncProgress`] events by wallet id and hands the
//! page a fresh [`LocalSyncState`] snapshot on every update.

use gpui::{Context, Subscription};

use crate::state::{store, StoreEvent, SyncProgress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSyncStatus {
    Idle,
    Active,
    Complete,
    /// Reserved: the sync surface does not emit error events yet (the
    /// React hook never produced this state either).
    #[allow(dead_code)]
    Error,
}

#[derive(Debug, Clone)]
pub struct LocalSyncState {
    pub status: LocalSyncStatus,
    /// `connecting` | `fetching_history` | `persisting` | `complete`.
    pub phase: Option<String>,
    pub percent: u32,
    /// Cleared when a fresh sync starts.
    #[allow(dead_code)]
    pub error: Option<String>,
}

impl LocalSyncState {
    pub fn idle() -> Self {
        Self {
            status: LocalSyncStatus::Idle,
            phase: None,
            percent: 0,
            error: None,
        }
    }

    fn from_progress(p: &SyncProgress) -> Self {
        Self {
            status: if p.phase == "complete" {
                LocalSyncStatus::Complete
            } else {
                LocalSyncStatus::Active
            },
            phase: Some(p.phase.clone()),
            percent: p.percent,
            error: None,
        }
    }
}

/// Initial state for `wallet_id`. The React hook started idle and only
/// saw events emitted after mount; the store keeps the last event per
/// wallet, so a sync that is still running when the page mounts (e.g. the
/// auto-sync kicked off by unlock just before navigating) is surfaced as
/// active instead of being missed. A finished sync stays idle, matching
/// the hook.
pub fn initial_sync_state<T: 'static>(wallet_id: &str, cx: &mut Context<T>) -> LocalSyncState {
    match store(cx).read(cx).sync_progress.get(wallet_id) {
        Some(p) if p.phase != "complete" => LocalSyncState::from_progress(p),
        _ => LocalSyncState::idle(),
    }
}

/// Subscribe to sync progress for one wallet. `on_update` runs with the new
/// snapshot each time a progress event for `wallet_id` arrives.
pub fn watch_sync<T: 'static>(
    wallet_id: String,
    cx: &mut Context<T>,
    mut on_update: impl FnMut(&mut T, LocalSyncState, &mut Context<T>) + 'static,
) -> Subscription {
    let st = store(cx);
    cx.subscribe(&st, move |this, _, ev: &StoreEvent, cx| {
        if let StoreEvent::SyncProgress(p) = ev {
            if p.wallet_id == wallet_id {
                on_update(this, LocalSyncState::from_progress(p), cx);
            }
        }
    })
}
