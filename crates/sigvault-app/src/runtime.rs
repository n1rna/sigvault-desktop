//! Bridge between GPUI's main-thread executor and the tokio runtime the core
//! logic expects.
//!
//! Every `cmd_*` function from `sigvault-core` is spawned onto a dedicated
//! multi-threaded tokio runtime; the returned future is a plain
//! `tokio::task::JoinHandle`, which can be awaited from a GPUI task. That
//! keeps blocking work (USB, Electrum, file I/O, Argon2) off the UI thread.

use std::future::Future;

use gpui::{App, Global};
use sigvault_core::{AppHandle, ApplicationState};

pub struct Backend {
    rt: tokio::runtime::Runtime,
    pub app: AppHandle,
    pub state: ApplicationState,
}

impl Global for Backend {}

impl Backend {
    pub fn new(app: AppHandle) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("sigvault-core")
            .build()
            .expect("failed to start tokio runtime");
        Self {
            rt,
            app,
            state: ApplicationState::new(),
        }
    }

    /// Run a core call on the tokio runtime. The closure receives owned
    /// clones of the host handle and application state; wrap the state in
    /// `State::new(&state)` to call a `cmd_*` function.
    ///
    /// ```ignore
    /// let task = backend(cx).call(|app, st| async move {
    ///     cmd_local_list_wallets(app, State::new(&st)).await
    /// });
    /// ```
    pub fn call<F, Fut, T>(&self, f: F) -> impl Future<Output = Result<T, String>> + 'static
    where
        F: FnOnce(AppHandle, ApplicationState) -> Fut,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
        T: Send + 'static,
    {
        let handle = self.rt.spawn(f(self.app.clone(), self.state.clone()));
        async move {
            match handle.await {
                Ok(result) => result,
                Err(e) => Err(format!("background task failed: {e}")),
            }
        }
    }

    /// Like [`Backend::call`] for futures that don't return a `Result`.
    pub fn spawn<Fut, T>(&self, fut: Fut) -> impl Future<Output = Option<T>> + 'static
    where
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let handle = self.rt.spawn(fut);
        async move { handle.await.ok() }
    }
}

/// Shorthand for `cx.global::<Backend>()`.
pub fn backend(cx: &App) -> &Backend {
    cx.global::<Backend>()
}
