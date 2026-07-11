//! App-mode selection commands.
//!
//! `cmd_set_app_mode` is what the Welcome page and the topbar context
//! switcher call when the user picks Cloud or Local; it persists the
//! selection (so a returning user lands in the same context on next launch)
//! and routes the frontend to the appropriate next page.

use log::info;
use tauri::{AppHandle, Manager, State};

use crate::app_mode::AppMode;
use crate::state::ApplicationState;
use crate::storage::EnvStorage;
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::types::CommandResult;

/// Persist the chosen mode and update the routing accordingly.
/// - Cloud → fall through to the existing init flow (env select / login /
///   dashboard) by re-invoking `cmd_initialize_app`.
/// - Local → route directly to the local wallet list (no auth, no env
///   selection in v1).
#[tauri::command]
pub async fn cmd_set_app_mode(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
    mode: AppMode,
) -> Result<CommandResult, String> {
    info!("Setting app mode to {:?}", mode);

    app_state.set_app_mode(mode).await;

    // Persist alongside the env selection (the env stays untouched).
    let env_storage = EnvStorage::new(app.clone());
    let mut stored = env_storage.load().await.unwrap_or_default();
    stored.app_mode = Some(mode);
    if let Err(e) = env_storage.store(&stored).await {
        return Err(format!("Failed to persist app mode: {e}"));
    }

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    match mode {
        AppMode::Local => {
            // Local mode never goes through set_environment, so the HW
            // manager isn't lazily initialized by the cloud path. Ensure
            // it exists so cmd_discover_hardware_wallets et al work for
            // standalone-wallet creation (QBL-220).
            app_state.ensure_hw_manager_for_local().await;
            update_state(
                &window,
                StateUpdateEvent::builder()
                    .app_mode(mode)
                    .route(WindowApplicationRoute::LocalWallets)
                    .build(),
            )
            .await
            .map_err(|e| format!("Failed to update state: {e}"))?;
            Ok(CommandResult::success("Local mode selected"))
        }
        AppMode::Cloud => {
            // Push the mode update first so the frontend has it before the
            // existing init flow starts emitting routing events.
            update_state(&window, StateUpdateEvent::builder().app_mode(mode).build())
                .await
                .map_err(|e| format!("Failed to update state: {e}"))?;
            // Re-enter the existing init flow: it'll route to SelectEnv if
            // no env is chosen yet, Login if env+no token, MainPage otherwise.
            super::init::cmd_initialize_app(app.clone(), app_state).await
        }
    }
}
