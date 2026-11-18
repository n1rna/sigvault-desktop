//! SigVault core: wallet, hardware-device, cloud-session and storage logic.
//!
//! These modules are carried over from the Tauri build (`src-tauri/src`)
//! with only their Tauri imports re-pointed at [`host`], so behaviour —
//! storage formats, wire protocols, signing pipelines — is identical. The
//! GPUI front end (`sigvault-app`) drives the `cmd_*` functions directly
//! and consumes the events they emit through the host channel.

pub mod api;
pub mod app_mode;
pub mod commands;
pub mod config;
pub mod env_config;
pub mod error;
pub mod host;
pub mod hwi;
pub mod kdf;
pub mod local_wallet;
pub mod machine;
pub mod oauth;
pub mod state;
pub mod storage;
pub mod websocket;
pub mod window;

pub use host::{AppHandle, HostEvent, State};
pub use state::ApplicationState;

/// One-time process setup the Tauri `run()` used to do before building the
/// app: install rustls' ring provider (electrum-client, reqwest and
/// tokio-tungstenite otherwise panic on first TLS handshake when more than
/// one provider is linked).
pub fn init_process() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
