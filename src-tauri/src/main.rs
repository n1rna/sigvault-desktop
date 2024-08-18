// src/main.rs

mod api_handler;
mod machine;
mod window;
mod ws_handler;

use crate::window::{
    send_backend_command, set_window_application_state, WindowApplicationRoute,
    WindowApplicationState,
};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State, Window};
use tokio::sync::Mutex;

use api_handler::ApiHandler;
use ws_handler::WebsocketHandler;

#[derive(Serialize, Deserialize, Debug)]
enum AppError {
    #[serde(rename = "error_websocket_already_active")]
    WebsocketAlreadyActive,
    #[serde(rename = "error_authorization_failed")]
    AuthorizationFailed,
    #[serde(rename = "error_empty_token")]
    EmptyToken,
    #[serde(rename = "error_machine_authorization_failed")]
    MachineAuthorizationFailed,
    #[serde(rename = "error_machine_not_registered")]
    MachineNotRegistered,
    #[serde(rename = "error_websocket_connection")]
    WebsocketConnection,
}

#[derive(Default, Clone)]
struct AuthTokens {
    auth_session: Option<String>,
    user_auth_token: Option<String>,
    machine_auth_token: Option<String>,
}

struct ApplicationState {
    ws_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    auth_tokens: Arc<Mutex<AuthTokens>>,
}

#[derive(Serialize, Deserialize)]
struct CommandResult {
    success: bool,
    message: String,
    error: Option<AppError>,
}

#[tokio::main]
#[tauri::command]
async fn cmd_register_new_machine(
    app_state: State<'_, ApplicationState>,
    machine_id: String,
    machine_name: String,
    machine_type: String,
) -> CommandResult {
    let api_handler = ApiHandler::new("http://localhost:8000".to_string());

    let machine_info = machine::get_machine_information();

    if machine_id != machine_info.machine_id || machine_type != machine_info.machine_type {
        return CommandResult {
            success: false,
            message: "Machine ID does not match".into(),
            error: None,
        };
    }

    match api_handler
        .register_new_machine(
            app_state
                .auth_tokens
                .lock()
                .await
                .user_auth_token
                .clone()
                .unwrap(),
            machine_id.clone(),
            machine_name.clone(),
            machine_type.clone(),
        )
        .await
    {
        Ok(_) => CommandResult {
            success: true,
            message: "Machine registered successfully".into(),
            error: None,
        },
        Err(e) => {
            error!("Failed to authorize machine websocket connection: {:?}", e);
            return CommandResult {
                success: false,
                message: "Machine authorization failed".into(),
                error: Some(AppError::MachineAuthorizationFailed),
            };
        }
    };

    CommandResult {
        success: true,
        message: "Machine registered successfully".into(),
        error: None,
    }
}

#[tokio::main]
#[tauri::command]
async fn cmd_start_websocket_connection(
    window: Window,
    app_state: State<'_, ApplicationState>,
    auth_session: String,
) -> CommandResult {
    set_window_application_state(
        &window,
        &WindowApplicationState {
            route: WindowApplicationRoute::MainPage,
            socket_connected: false,
        },
    );

    let ws_thread = app_state.ws_thread.lock().await;

    if let Some(join_handle) = ws_thread.as_ref() {
        if !join_handle.inner().is_finished() {
            set_window_application_state(&window, &WindowApplicationState{
                route: WindowApplicationRoute::MainPage,
                socket_connected: true,
            });
        
            return CommandResult {
                success: false,
                message: "Websocket connection already started".into(),
                error: Some(AppError::WebsocketAlreadyActive),
            };
        }
    }

    drop(ws_thread); // Release the lock

    // Store the auth session
    app_state.auth_tokens.lock().await.auth_session = Some(auth_session.clone());

    let api_handler = ApiHandler::new("http://localhost:8000".to_string());

    // Authorize user
    let user_auth = match api_handler
        .authorize_user_websocket_connection(auth_session)
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to authorize user websocket connection: {:?}", e);
            return CommandResult {
                success: false,
                message: "Authorization failed".into(),
                error: Some(AppError::AuthorizationFailed),
            };
        }
    };

    if user_auth.token.is_empty() {
        return CommandResult {
            success: false,
            message: "Authorization failed".into(),
            error: Some(AppError::AuthorizationFailed),
        };
    }

    // Store the user auth token
    app_state.auth_tokens.lock().await.user_auth_token = Some(user_auth.token.clone());

    let machine_info = machine::get_machine_information();

    // Authorize machine
    let machine_auth = match api_handler
        .authorize_machine_websocket_connection(user_auth.token, &machine_info)
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to authorize machine websocket connection: {:?}", e);
            return CommandResult {
                success: false,
                message: "Machine authorization failed".into(),
                error: Some(AppError::MachineAuthorizationFailed),
            };
        }
    };

    if machine_auth.status == "unregistered" {
        set_window_application_state(
            &window,
            &WindowApplicationState {
                route: WindowApplicationRoute::MachineRegistration,
                socket_connected: false,
            },
        );

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        send_backend_command(
            &window,
            "register_machine".into(),
            serde_json::json!({
                "machine_id": machine_info.machine_id.clone(),
                "machine_type": machine_info.machine_type.clone(),
            }),
        );
        return CommandResult {
            success: false,
            message: "Machine not registered".into(),
            error: Some(AppError::MachineNotRegistered),
        };
    }

    info!("machine auth {:?}", machine_auth);

    if machine_auth.token.is_none() {
        return CommandResult {
            success: false,
            message: "Machine authorization failed".into(),
            error: Some(AppError::MachineAuthorizationFailed),
        };
    }
    let machine_token = machine_auth.token.unwrap();
    let session_id = machine_auth.session_id.unwrap();

    info!("Machine token: {}", machine_token);
    // Store the machine auth token
    app_state.auth_tokens.lock().await.machine_auth_token = Some(machine_token.clone());

    let join_handler = tauri::async_runtime::spawn(async move {
        let ws_handler = WebsocketHandler::new(window.clone(), "ws://localhost:8000".to_string());
        if let Err(e) = ws_handler.run(machine_token, session_id).await {
            error!("WebSocket connection error: {:?}", e);
            set_window_application_state(
                &window,
                &WindowApplicationState {
                    route: WindowApplicationRoute::MainPage,
                    socket_connected: false,
                },
            );
        }
    });

    app_state.ws_thread.lock().await.replace(join_handler);

    CommandResult {
        success: true,
        message: "Command executed successfully".into(),
        error: None,
    }
}

#[tauri::command]
async fn cmd_close_splashscreen(window: Window) {
    info!("Attempting to close splashscreen");
    match window.get_window("splashscreen") {
        Some(splashscreen) => {
            debug!("Splashscreen window found, closing");
            if let Err(e) = splashscreen.close() {
                error!("Failed to close splashscreen: {:?}", e);
            } else {
                info!("Splashscreen closed successfully");
            }
        }
        None => {
            warn!("No splashscreen window found");
        }
    }

    info!("Attempting to show main window");
    match window.get_window("main") {
        Some(main_window) => {
            debug!("Main window found, showing");
            if let Err(e) = main_window.show() {
                error!("Failed to show main window: {:?}", e);
            } else {
                info!("Main window shown successfully");
            }
        }
        None => {
            error!("No window labeled 'main' found");
        }
    }
}

fn main() {
    env_logger::init();
    info!("Starting application");

    tauri::Builder::default()
        .manage(ApplicationState {
            ws_thread: Arc::new(Mutex::new(None)),
            auth_tokens: Arc::new(Mutex::new(AuthTokens::default())),
        })
        .invoke_handler(tauri::generate_handler![
            cmd_start_websocket_connection,
            cmd_register_new_machine,
            cmd_close_splashscreen,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Application exited");
}
