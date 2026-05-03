//! App-mode selection commands.
//!
//! `cmd_set_app_mode` is what the pre-login mode-chooser screen calls when
//! the user picks Cloud or Local; it persists the selection (so the
//! chooser doesn't show again on next launch) and routes the frontend to
//! the appropriate next page. `cmd_clear_app_mode` is the inverse, called
//! from the settings "Switch mode" action.

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

/// Forget the persisted mode so the next `cmd_initialize_app` (or this
/// call's own routing) returns the user to the mode chooser. Does NOT
/// clear local wallets, OAuth tokens, or the selected environment — those
/// are independent.
#[tauri::command]
pub async fn cmd_clear_app_mode(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    info!("Clearing app mode (returning to mode chooser)");

    app_state.clear_app_mode().await;

    let env_storage = EnvStorage::new(app.clone());
    let mut stored = env_storage.load().await.unwrap_or_default();
    stored.app_mode = None;
    if let Err(e) = env_storage.store(&stored).await {
        return Err(format!("Failed to clear app mode: {e}"));
    }

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    update_state(
        &window,
        StateUpdateEvent::builder()
            .route(WindowApplicationRoute::ModeChooser)
            .build(),
    )
    .await
    .map_err(|e| format!("Failed to update state: {e}"))?;

    Ok(CommandResult::success("App mode cleared"))
}
