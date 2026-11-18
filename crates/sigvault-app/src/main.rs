//! SigVault Desktop — GPUI edition.
//!
//! Boot sequence mirrors the Tauri app: install the TLS provider, start the
//! core (on its own tokio runtime), open the window, start draining core
//! events into the [`state::AppStore`], then call `cmd_initialize_app`,
//! which routes to Welcome / SelectEnv / Login / Sessions / LocalWallets.

mod assets;
mod pages;
mod router;
mod runtime;
mod shell;
mod state;
mod theme;
mod ui;

use futures::StreamExt;
use gpui::{
    point, px, size, App, AppContext, Application, Bounds, KeyBinding, TitlebarOptions,
    WindowBounds, WindowOptions,
};
use sigvault_core::commands::cmd_initialize_app;
use sigvault_core::{AppHandle, State};

use crate::runtime::{backend, Backend};

gpui::actions!(sigvault, [Quit]);

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "info,sigvault_core=debug,sigvault_desktop=debug",
    ))
    .init();
    log::info!("Starting SigVault Desktop (GPUI)");

    sigvault_core::init_process();

    let data_dir = std::env::var_os("SIGVAULT_DATA_DIR")
        .map(Into::into)
        .unwrap_or_else(AppHandle::default_data_dir);
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::error!("cannot create data dir {}: {e}", data_dir.display());
    }
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let host = AppHandle::new(data_dir, tx);

    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            cx.set_global(Backend::new(host));
            let store = state::init(cx);
            ui::input::bind_keys(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            #[cfg(target_os = "macos")]
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            #[cfg(not(target_os = "macos"))]
            cx.bind_keys([KeyBinding::new("ctrl-q", Quit, None)]);
            cx.set_menus(vec![gpui::Menu {
                name: "SigVault".into(),
                items: vec![gpui::MenuItem::action("Quit SigVault", Quit)],
            }]);

            let bounds = Bounds::centered(None, size(px(900.), px(720.)), cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(TitlebarOptions {
                            title: Some("SigVault".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(16.), px(20.))),
                        }),
                        window_min_size: Some(size(px(760.), px(560.))),
                        app_id: Some(sigvault_core::host::APP_IDENTIFIER.into()),
                        ..Default::default()
                    },
                    |window, cx| cx.new(|cx| shell::Root::new(window, cx)),
                )
                .expect("failed to open window");
            window
                .update(cx, |_, window, _| window.activate_window())
                .ok();
            cx.activate(true);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Event pump: core → store (the `listen("app_event")` of App.tsx).
            cx.spawn(async move |cx| {
                while let Some(ev) = rx.next().await {
                    if store.update(cx, |s, cx| s.apply(ev, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();

            // Listener is ready → initialize (routes onward).
            let init = backend(cx).call(|app, st| async move {
                cmd_initialize_app(app, State::new(&st)).await
            });
            cx.spawn(async move |_| {
                if let Err(e) = init.await {
                    log::error!("initialize failed: {e}");
                }
            })
            .detach();
        });
}
