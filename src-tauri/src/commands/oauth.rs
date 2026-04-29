// OAuth authentication commands with PKCE and CSRF protection.
//
// A fresh `OAuthState` is built for every `cmd_authenticate` call so that
// (a) PKCE/CSRF tokens are not reused across login attempts and
// (b) the authorization URL reflects the currently selected environment.

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    response::IntoResponse,
    routing::get,
    Router,
};
use log::{error, info};
use oauth2::{
    reqwest::async_http_client, AuthorizationCode, CsrfToken, PkceCodeVerifier, Scope,
    TokenResponse,
};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::api::ApiClient;
use crate::error::AppErrorCode;
use crate::oauth::OAuthState;
use crate::state::ApplicationState;
use crate::storage::SecureStorage;
use crate::window::{update_state, StateUpdateEvent, WindowApplicationRoute};

use super::types::CommandResult;

/// Callback query parameters from OAuth provider
#[derive(Deserialize)]
struct CallbackQuery {
    code: AuthorizationCode,
    state: CsrfToken,
}

/// Launch the user's default browser at `url`. Cross-platform.
fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // LD_LIBRARY_PATH from an AppImage bundle can break xdg-open.
        std::process::Command::new("xdg-open")
            .arg(url)
            .env_remove("LD_LIBRARY_PATH")
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
    }
}

async fn current_oauth_flow(app_state: &ApplicationState) -> Result<OAuthState, String> {
    app_state
        .oauth_flow
        .read()
        .await
        .clone()
        .ok_or_else(|| "OAuth flow not initialized".to_string())
}

async fn authorize_callback(
    Extension(app_state): Extension<ApplicationState>,
    query: Query<CallbackQuery>,
) -> impl IntoResponse {
    info!("Received OAuth callback");

    let oauth_state = match current_oauth_flow(&app_state).await {
        Ok(s) => s,
        Err(e) => {
            error!("OAuth callback received without active flow: {e}");
            return "Authorization failed: no active flow".to_string();
        }
    };

    if query.state.secret() != oauth_state.csrf_token.secret() {
        error!("CSRF token mismatch - possible MITM attack!");
        return "Authorization failed: invalid state".to_string();
    }

    *oauth_state.auth_code.lock().await = Some(query.code.secret().to_string());

    info!("OAuth authorization code received successfully");
    "Authorization successful! You can close this window and return to the application.".to_string()
}

async fn run_oauth_server(
    app_state: ApplicationState,
    socket_addr: std::net::SocketAddr,
) -> Result<(), String> {
    info!("Starting OAuth callback server on {socket_addr}");

    let app = Router::new()
        .route("/callback", get(authorize_callback))
        .layer(Extension(app_state));

    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .map_err(|e| format!("Failed to bind to {socket_addr}: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn cmd_authenticate(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    app_state.require_cloud_mode().await?;
    info!("Starting OAuth authentication flow");

    // Refuse to start auth without a chosen environment so we don't end up
    // with a token pinned to no backend.
    let env = app_state.require_env().await?;

    // Build a fresh OAuth flow for this attempt: new PKCE + CSRF, and an
    // auth URL pointing at the currently selected environment.
    let new_flow = OAuthState::new(
        env!("OAUTH2_CLIENT_ID").to_string(),
        env.resolved_auth_url(),
        env!("OAUTH2_TOKEN_URL").to_string(),
    )
    .map_err(|e| format!("Failed to build OAuth flow: {e}"))?;

    *app_state.oauth_flow.write().await = Some(new_flow.clone());

    let (auth_url, _) = new_flow
        .client
        .authorize_url(|| new_flow.csrf_token.clone())
        .add_scope(Scope::new("openid".to_string()))
        .set_pkce_challenge(new_flow.pkce_challenge.clone())
        .url();

    info!("Opening browser for authentication: {auth_url}");

    let server_state = app_state.inner().clone();
    let socket_addr = new_flow.socket_addr;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_oauth_server(server_state, socket_addr).await {
            error!("OAuth server error: {e}");
        }
    });

    open_browser(auth_url.as_str())
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    let auth_code = wait_for_auth_code(new_flow.auth_code.clone()).await?;

    info!("Exchanging authorization code for token");

    let pkce_verifier = new_flow
        .pkce_verifier
        .lock()
        .await
        .clone()
        .ok_or("PKCE verifier not found")?;

    info!("Sending token exchange request to Zitadel");
    let token_result = new_flow
        .client
        .exchange_code(AuthorizationCode::new(auth_code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(async_http_client)
        .await;

    match &token_result {
        Ok(_) => info!("Token exchange successful"),
        Err(e) => error!("Token exchange error: {e:?}"),
    }

    let token_response = token_result
        .map_err(|e| format!("Token exchange failed: {e}"))?;

    let access_token = token_response.access_token().secret().to_string();
    info!("Successfully obtained access token");

    // Drop the flow now that we have the token; nothing else needs it.
    *app_state.oauth_flow.write().await = None;

    let storage = SecureStorage::new(app.clone());
    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {e}");
        return Ok(CommandResult::error(
            "Failed to initialize secure storage",
            AppErrorCode::AuthorizationFailed,
        ));
    }

    if let Err(e) = storage
        .update_oauth_access_token(access_token.clone())
        .await
    {
        error!("Failed to store OAuth access token: {e}");
    } else {
        info!("OAuth access token stored successfully");
    }

    authenticate_user(app, app_state.inner().clone(), access_token).await
}

/// Authenticate user with OAuth access token
/// Fetches user profile, updates app state, and navigates to main page
/// On failure, cleans up the stored OAuth token
pub(super) async fn authenticate_user(
    app: AppHandle,
    app_state: ApplicationState,
    access_token: String,
) -> Result<CommandResult, String> {
    info!("Authenticating user with access token");

    let storage = SecureStorage::new(app.clone());
    let api_base_url = app_state.require_api_base_url().await?;
    let api_client = ApiClient::new(api_base_url);

    let user_profile = match api_client.user_profile(access_token.clone()).await {
        Ok(profile) => {
            info!("User profile fetched successfully: {:?}", profile.id);
            Ok(profile)
        }
        Err(e) => {
            error!("Failed to fetch user profile: {e:?}");
            error!("Cleaning up invalid OAuth token from storage");

            if let Err(e) = storage.clear_auth_data().await {
                error!("Failed to clear auth data: {e}");
            }

            app_state.auth_tokens.lock().await.clear();
            app_state.user_data.lock().await.clear();
            app_state.remote_sessions.lock().await.clear();

            Err(CommandResult::error(
                "Failed to fetch user profile",
                AppErrorCode::AuthorizationFailed,
            ))
        }
    };

    app_state
        .auth_tokens
        .lock()
        .await
        .set_oauth_access_token(access_token);

    let mut authenticated = false;
    let mut route = WindowApplicationRoute::Login;
    match user_profile {
        Ok(profile) => {
            app_state.user_data.lock().await.set_from_profile(profile);
            authenticated = true;
            route = WindowApplicationRoute::MainPage;
        }
        Err(_) => {
            error!("User profile is not available, cannot set user data in state");
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        update_state(
            &window,
            StateUpdateEvent::builder()
                .authenticated(authenticated)
                .route(route)
                .socket_connected(false)
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(CommandResult::success("Authentication finished"))
}

async fn wait_for_auth_code(auth_code: Arc<Mutex<Option<String>>>) -> Result<String, String> {
    for _ in 0..300 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let code = auth_code.lock().await;
        if let Some(code) = code.as_ref() {
            return Ok(code.clone());
        }
    }

    Err("Authentication timeout: no callback received".to_string())
}

#[tauri::command]
pub async fn cmd_logout(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    app_state.require_cloud_mode().await?;
    info!("Logging out");

    let storage = SecureStorage::new(app.clone());

    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {e}");
    }

    if let Err(e) = storage.clear_auth_data().await {
        error!("Failed to clear auth data: {e}");
    }

    app_state.auth_tokens.lock().await.clear();
    app_state.user_data.lock().await.clear();
    app_state.remote_sessions.lock().await.clear();
    *app_state.oauth_flow.write().await = None;

    if let Some(window) = app.get_webview_window("main") {
        update_state(
            &window,
            StateUpdateEvent::builder()
                .authenticated(false)
                .route(WindowApplicationRoute::Login)
                .build(),
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(CommandResult::success("Logged out successfully"))
}
