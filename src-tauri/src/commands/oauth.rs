// OAuth authentication commands with PKCE and CSRF protection

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

/// Callback handler for OAuth redirect
async fn authorize_callback(
    handle: Extension<AppHandle>,
    query: Query<CallbackQuery>,
) -> impl IntoResponse {
    info!("Received OAuth callback");

    let oauth_state = handle.state::<OAuthState>();

    // Verify CSRF token
    if query.state.secret() != oauth_state.csrf_token.secret() {
        error!("CSRF token mismatch - possible MITM attack!");
        return "Authorization failed: invalid state".to_string();
    }

    // Store the authorization code for the main authenticate command to use
    let mut auth_code = oauth_state.auth_code.lock().await;
    *auth_code = Some(query.code.secret().to_string());

    info!("OAuth authorization code received successfully");
    "Authorization successful! You can close this window and return to the application.".to_string()
}

/// Run the local OAuth callback server
async fn run_oauth_server(handle: AppHandle) -> Result<(), String> {
    let oauth_state = handle.state::<OAuthState>();
    let socket_addr = oauth_state.socket_addr;

    info!("Starting OAuth callback server on {socket_addr}");

    let app = Router::new()
        .route("/callback", get(authorize_callback))
        .layer(Extension(handle.clone()));

    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .map_err(|e| format!("Failed to bind to {socket_addr}: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}

/// Main authenticate command - initiates OAuth flow and handles the complete authentication
#[tauri::command]
pub async fn cmd_authenticate(
    app: AppHandle,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    info!("Starting OAuth authentication flow");

    let oauth_state = app.state::<OAuthState>();

    // Generate authorization URL with PKCE challenge
    let (auth_url, _) = oauth_state
        .client
        .authorize_url(|| oauth_state.csrf_token.clone())
        .add_scope(Scope::new("openid".to_string()))
        .set_pkce_challenge(oauth_state.pkce_challenge.clone())
        .url();

    info!("Opening browser for authentication: {auth_url}");

    // Spawn the OAuth callback server
    let server_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_oauth_server(server_handle).await {
            error!("OAuth server error: {e}");
        }
    });

    // Open the authorization URL in the user's browser.
    // Clear LD_LIBRARY_PATH so AppImage-bundled libs don't break xdg-open.
    std::process::Command::new("xdg-open")
        .arg(auth_url.to_string())
        .env_remove("LD_LIBRARY_PATH")
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    // Wait for the callback (poll for auth code)
    let auth_code = wait_for_auth_code(oauth_state.auth_code.clone()).await?;

    info!("Exchanging authorization code for token");

    // Exchange authorization code for access token
    let pkce_verifier = oauth_state
        .pkce_verifier
        .lock()
        .await
        .clone()
        .ok_or("PKCE verifier not found")?;

    info!("Sending token exchange request to Zitadel");
    let token_result = oauth_state
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

    // Initialize secure storage
    let storage = SecureStorage::new(app.clone());
    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {e}");
        return Ok(CommandResult::error(
            "Failed to initialize secure storage",
            AppErrorCode::AuthorizationFailed,
        ));
    }

    // Store the OAuth access token in secure storage
    if let Err(e) = storage
        .update_oauth_access_token(access_token.clone())
        .await
    {
        error!("Failed to store OAuth access token: {e}");
    } else {
        info!("OAuth access token stored successfully");
    }

    // Authenticate user with the access token (shared logic)
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
    let api_client = ApiClient::new();

    // Fetch user profile to verify token and get user data
    let user_profile = match api_client.user_profile(access_token.clone()).await {
        Ok(profile) => {
            info!("User profile fetched successfully: {:?}", profile.id);
            Ok(profile)
        }
        Err(e) => {
            error!("Failed to fetch user profile: {e:?}");
            error!("Cleaning up invalid OAuth token from storage");

            // Clean up the invalid token from secure storage
            if let Err(e) = storage.clear_auth_data().await {
                error!("Failed to clear auth data: {e}");
            }

            // Clear in-memory state
            app_state.auth_tokens.lock().await.clear();
            app_state.user_data.lock().await.clear();
            app_state.remote_sessions.lock().await.clear();

            Err(CommandResult::error(
                "Failed to fetch user profile",
                AppErrorCode::AuthorizationFailed,
            ))
        }
    };

    // Store OAuth token in app state
    app_state
        .auth_tokens
        .lock()
        .await
        .set_oauth_access_token(access_token);

    // Store user profile in app state
    let mut authenticated = false; // This will be set to true after successful authentication
    let mut route = WindowApplicationRoute::Login; // Default to login route on failure
    match user_profile {
        Ok(profile) => {
            app_state.user_data.lock().await.set_from_profile(profile);
            authenticated = true;
            route = WindowApplicationRoute::MainPage;
        }
        Err(_) => {
            // This case should not happen since we return early on profile fetch failure
            error!("User profile is not available, cannot set user data in state");
        }
    }

    // Update window state to show logged in
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

/// Wait for the authorization code from the callback
async fn wait_for_auth_code(auth_code: Arc<Mutex<Option<String>>>) -> Result<String, String> {
    // Poll for up to 5 minutes
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
    info!("Logging out");

    let storage = SecureStorage::new(app.clone());

    // Initialize stronghold
    if let Err(e) = storage.initialize(b"sigvault_default_key").await {
        error!("Failed to initialize storage: {e}");
    }

    // Clear stored auth data
    if let Err(e) = storage.clear_auth_data().await {
        error!("Failed to clear auth data: {e}");
    }

    // Clear in-memory tokens, user data, and remote sessions
    app_state.auth_tokens.lock().await.clear();
    app_state.user_data.lock().await.clear();
    app_state.remote_sessions.lock().await.clear();

    // Navigate to loading/login
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
