//! Chain backend for local wallets: Esplora (HTTP) or Electrum (TCP/TLS),
//! picked by the configured URL's scheme.
//!
//! - `http://` / `https://` → Esplora REST API (what sigvault hosts at
//!   `https://esplora.<env>.sigvault.org`; also mempool.space-style servers).
//! - `tcp://` / `ssl://` / bare `host:port` → Electrum protocol, for users who
//!   point the app at their own Electrum server.
//!
//! Sigvault's own servers sit behind Cloudflare, which only proxies HTTP, so
//! the hosted defaults are Esplora. All calls here block; callers run them on
//! the blocking pool.

use bdk_esplora::esplora_client;
use bdk_esplora::EsploraExt;
use bdk_wallet::bitcoin::Transaction;
use bdk_wallet::chain::spk_client::FullScanResponse;
use bdk_wallet::{KeychainKind, Update, Wallet};
use wallet_runtime::ElectrumClient;

/// Per-request timeout for Esplora calls, in seconds. Keeps a dead or
/// unreachable server from leaving a sync hanging.
const ESPLORA_TIMEOUT_SECS: u64 = 30;

/// Concurrent requests an Esplora full scan may have in flight.
const ESPLORA_PARALLEL_REQUESTS: usize = 4;

/// Which protocol a URL selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Esplora,
    Electrum,
}

impl BackendKind {
    pub fn for_url(url: &str) -> Self {
        let lower = url.trim().to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            BackendKind::Esplora
        } else {
            BackendKind::Electrum
        }
    }
}

pub enum ChainClient {
    Esplora(esplora_client::BlockingClient),
    Electrum(Box<ElectrumClient>),
}

impl ChainClient {
    /// Open a client for `url`. For Esplora this also fetches the tip height,
    /// so an unreachable or wrong server fails here (the "connecting" phase)
    /// rather than midway through a scan.
    pub fn connect(url: &str) -> Result<Self, String> {
        match BackendKind::for_url(url) {
            BackendKind::Esplora => {
                let base = url.trim().trim_end_matches('/');
                let client = esplora_client::Builder::new(base)
                    .timeout(ESPLORA_TIMEOUT_SECS)
                    .header(
                        "User-Agent",
                        concat!("sigvault-desktop/", env!("CARGO_PKG_VERSION")),
                    )
                    .build_blocking();
                client
                    .get_height()
                    .map_err(|e| format!("esplora {base}: {e}"))?;
                Ok(ChainClient::Esplora(client))
            }
            BackendKind::Electrum => ElectrumClient::connect(url)
                .map(|c| ChainClient::Electrum(Box::new(c)))
                .map_err(|e| e.to_string()),
        }
    }

    /// Full scan of every keychain up to `stop_gap` unused addresses.
    pub fn full_scan(&self, wallet: &Wallet, stop_gap: usize) -> Result<Update, String> {
        match self {
            ChainClient::Esplora(client) => {
                let request = wallet.start_full_scan().build();
                let response: FullScanResponse<KeychainKind> = client
                    .full_scan(request, stop_gap, ESPLORA_PARALLEL_REQUESTS)
                    .map_err(|e| format!("esplora full_scan: {e}"))?;
                Ok(response.into())
            }
            ChainClient::Electrum(client) => client
                .full_scan(wallet, stop_gap, 5, /* fetch_prev_txouts */ true)
                .map_err(|e| e.to_string()),
        }
    }

    /// Broadcast `tx`; returns its txid.
    pub fn broadcast(&self, tx: &Transaction) -> Result<String, String> {
        match self {
            ChainClient::Esplora(client) => client
                .broadcast(tx)
                .map(|()| tx.compute_txid().to_string())
                .map_err(|e| format!("esplora broadcast: {e}")),
            ChainClient::Electrum(client) => client.broadcast(tx).map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_selects_backend() {
        for url in [
            "https://esplora.regtest.sigvault.org",
            "HTTPS://example.org/api/",
            "http://localhost:3002",
        ] {
            assert_eq!(BackendKind::for_url(url), BackendKind::Esplora, "{url}");
        }
        for url in [
            "ssl://electrum.example.org:50002",
            "tcp://127.0.0.1:60401",
            "127.0.0.1:60401",
        ] {
            assert_eq!(BackendKind::for_url(url), BackendKind::Electrum, "{url}");
        }
    }
}
