// UI-related commands

use crate::host::{State, WebviewWindow};
use serde::Serialize;

use crate::state::ApplicationState;

#[derive(Serialize)]
pub struct UserInfo {
    pub id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
}

pub async fn cmd_get_current_user(
    app_state: State<'_, ApplicationState>,
) -> Result<UserInfo, String> {
    let user_data = app_state.user_data.lock().await;

    Ok(UserInfo {
        id: user_data.id.clone(),
        email: user_data.email.clone(),
        name: user_data.name.clone(),
        username: user_data.username.clone(),
    })
}

pub async fn cmd_get_remote_sessions(
    app_state: State<'_, ApplicationState>,
) -> Result<Vec<crate::api::types::RemoteSession>, String> {
    app_state.require_cloud_mode().await?;
    let remote_sessions = app_state.remote_sessions.lock().await;
    Ok(remote_sessions.clone())
}

pub async fn cmd_navigate(
    window: WebviewWindow,
    _app_state: State<'_, ApplicationState>,
    route: String,
) -> Result<(), String> {
    use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

    // Parse the route string to WindowApplicationRoute
    let app_route = match route.as_str() {
        "Loading" => WindowApplicationRoute::Loading,
        "Welcome" => WindowApplicationRoute::Welcome,
        "SelectEnv" => WindowApplicationRoute::SelectEnv,
        "Login" => WindowApplicationRoute::Login,
        "MainPage" => WindowApplicationRoute::MainPage,
        "MachineRegistration" => WindowApplicationRoute::MachineRegistration,
        "RemoteSessions" => WindowApplicationRoute::RemoteSessions,
        "SessionDetails" => WindowApplicationRoute::SessionDetails,
        "LocalWallets" => WindowApplicationRoute::LocalWallets,
        _ => return Err(format!("Invalid route: {route}")),
    };

    update_state(
        &window,
        StateUpdateEvent::builder().route(app_route).build(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
