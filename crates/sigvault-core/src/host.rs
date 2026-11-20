//! Host shim: the small slice of the Tauri runtime API the core logic was
//! written against (`AppHandle`, `WebviewWindow`, `State`, `emit`,
//! `async_runtime::spawn`), re-implemented without a webview.
//!
//! The core modules (`commands`, `local_wallet`, `hwi`, `websocket`, …) are
//! carried over from the Tauri app unchanged apart from their imports, so
//! this module is the only seam between the wallet/session logic and the
//! GPUI front end. Everything a command "emits to the webview" lands on an
//! unbounded channel as a [`HostEvent`]; the UI drains that channel on its
//! own executor and applies the events to its state model.

use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use futures::channel::mpsc::UnboundedSender;
use serde::Serialize;

/// Reverse-DNS identifier shared with the Tauri build, so both apps read
/// and write the same data directory (wallets, settings, env selection).
pub const APP_IDENTIFIER: &str = "org.sigvault.desktop";

/// An event emitted by the core toward the UI. `event` is the channel name
/// the Tauri build used (`app_event`, `local_wallet_sync_progress`, …) and
/// `payload` is the JSON the webview would have received.
#[derive(Debug, Clone)]
pub struct HostEvent {
    pub event: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct HostError(pub String);

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HostError {}

struct HostInner {
    data_dir: PathBuf,
    events: UnboundedSender<HostEvent>,
}

/// Replacement for `tauri::AppHandle`. Cheap to clone.
#[derive(Clone)]
pub struct AppHandle {
    inner: Arc<HostInner>,
}

impl AppHandle {
    pub fn new(data_dir: PathBuf, events: UnboundedSender<HostEvent>) -> Self {
        Self {
            inner: Arc::new(HostInner { data_dir, events }),
        }
    }

    /// Default per-user data dir, matching Tauri's `app_data_dir()`.
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_IDENTIFIER)
    }

    pub fn path(&self) -> PathResolver<'_> {
        PathResolver(self)
    }

    pub fn emit<S: Serialize>(&self, event: &str, payload: S) -> Result<(), HostError> {
        let payload = serde_json::to_value(payload).map_err(|e| HostError(e.to_string()))?;
        self.inner
            .events
            .unbounded_send(HostEvent {
                event: event.to_string(),
                payload,
            })
            .map_err(|e| HostError(e.to_string()))
    }

    /// There is exactly one window; any label resolves to it.
    pub fn get_webview_window(&self, _label: &str) -> Option<WebviewWindow> {
        Some(WebviewWindow { app: self.clone() })
    }
}

pub struct PathResolver<'a>(&'a AppHandle);

impl PathResolver<'_> {
    pub fn app_data_dir(&self) -> Result<PathBuf, HostError> {
        Ok(self.0.inner.data_dir.clone())
    }
}

/// Replacement for `tauri::WebviewWindow`: an emit target.
#[derive(Clone)]
pub struct WebviewWindow {
    app: AppHandle,
}

impl WebviewWindow {
    pub fn emit<S: Serialize>(&self, event: &str, payload: S) -> Result<(), HostError> {
        self.app.emit(event, payload)
    }

    pub fn get_webview_window(&self, label: &str) -> Option<WebviewWindow> {
        self.app.get_webview_window(label)
    }

    pub fn app_handle(&self) -> &AppHandle {
        &self.app
    }
}

impl fmt::Debug for WebviewWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebviewWindow")
    }
}

/// Replacement for `tauri::State`: a borrowed reference to managed state.
pub struct State<'r, T: Send + Sync + 'static>(&'r T);

impl<'r, T: Send + Sync + 'static> State<'r, T> {
    pub fn new(inner: &'r T) -> Self {
        Self(inner)
    }

    pub fn inner(&self) -> &'r T {
        self.0
    }
}

impl<T: Send + Sync + 'static> Clone for State<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for State<'_, T> {}

impl<T: Send + Sync + 'static> Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}

pub mod async_runtime {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Spawn onto the ambient tokio runtime (the app runs every command
    /// inside one, see `sigvault_app::runtime`).
    pub fn spawn<F>(fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        JoinHandle(tokio::spawn(fut))
    }

    pub struct JoinHandle<T>(tokio::task::JoinHandle<T>);

    impl<T> JoinHandle<T> {
        pub fn inner(&self) -> &tokio::task::JoinHandle<T> {
            &self.0
        }

        pub fn abort(&self) {
            self.0.abort();
        }
    }

    impl<T> Future for JoinHandle<T> {
        type Output = Result<T, tokio::task::JoinError>;
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut self.0).poll(cx)
        }
    }
}
