// src/main.rs

mod api_handler;
mod machine;
mod window;
mod ws_handler;

use crate::window::{
    create_shared_window_state, send_backend_command, set_window_application_state,
    WindowApplicationRoute, WindowApplicationState, WindowState,
};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};
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
    #[serde(rename = "error_fetch_remote_sessions_failed")]
    FetchRemoteSessionsFailed,
}

#[derive(Default, Clone)]
struct AuthTokens {
    auth_session: Option<String>,
    user_auth_token: Option<String>,
    machine_auth_token: Option<String>,
}

#[derive(Clone)]
struct ApplicationState {
    ws_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    ws_handler: Arc<Mutex<Option<WebsocketHandler>>>, // Add this line
    registration_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    auth_tokens: Arc<Mutex<AuthTokens>>,
    window_state: Arc<Mutex<WindowState>>,
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
    window: WebviewWindow,
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

    let auth_token = app_state.auth_tokens.clone();

    match api_handler
        .register_new_machine(
            auth_token.lock().await.user_auth_token.clone().unwrap(),
            machine_id.clone(),
            machine_name.clone(),
            machine_type.clone(),
        )
        .await
    {
        Ok(_) => {
            let window_clone = window.clone();
            let window_state_clone = app_state.window_state.clone();

            let registration_thread = tauri::async_runtime::spawn(async move {
                let mut window_state = window_state_clone.lock().await;
                set_window_application_state(
                    &window_clone,
                    &WindowApplicationState {
                        route: Some(WindowApplicationRoute::MainPage),
                        socket_connected: false,
                        current_session_id: None,
                        current_session_type: None,
                    },
                    &mut window_state,
                )
                .await
                .unwrap();
            });

            // Store the registration thread in the application state
            *app_state.registration_thread.lock().await = Some(registration_thread);
        }
        Err(e) => {
            error!("Failed to authorize machine websocket connection: {:?}", e);
            return CommandResult {
                success: false,
                message: "Machine authorization failed".into(),
                error: Some(AppError::MachineAuthorizationFailed),
            };
        }
    };

    info!("Returning command execution");

    CommandResult {
        success: true,
        message: "Command executed successfully".into(),
        error: None,
    }
}

#[tokio::main]
#[tauri::command]
async fn cmd_start_backend_authentication(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    auth_session: String,
) -> CommandResult {
    let mut window_state = app_state.window_state.lock().await;

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
                route: Some(WindowApplicationRoute::MachineRegistration),
                socket_connected: false,
                current_session_id: None,
                current_session_type: None,
            },
            &mut window_state,
        )
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        send_backend_command(
            &window,
            "register_machine".into(),
            serde_json::json!({
                "machine_id": machine_info.machine_id.clone(),
                "machine_type": machine_info.machine_type.clone(),
            }),
            &mut window_state,
        )
        .await
        .unwrap();
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

    info!("Machine token: {}", machine_token);
    // Store the machine auth token
    app_state.auth_tokens.lock().await.machine_auth_token = Some(machine_token.clone());

    // Fetch remote sessions
    let remote_sessions = match api_handler
        .fetch_remote_sessions(
            app_state
                .auth_tokens
                .lock()
                .await
                .user_auth_token
                .clone()
                .unwrap(),
        )
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to fetch remote sessions: {:?}", e);
            return CommandResult {
                success: false,
                message: "Failed to fetch remote sessions".into(),
                error: Some(AppError::FetchRemoteSessionsFailed),
            };
        }
    };

    // Send remote sessions to the frontend
    send_backend_command(
        &window,
        "update_remote_sessions".into(),
        serde_json::json!(remote_sessions),
        &mut window_state,
    )
    .await
    .unwrap();

    // Redirect to sessions page
    set_window_application_state(
        &window,
        &WindowApplicationState {
            route: Some(WindowApplicationRoute::RemoteSessions),
            socket_connected: false,
            current_session_id: None,
            current_session_type: None,
        },
        &mut window_state,
    )
    .await
    .unwrap();

    CommandResult {
        success: true,
        message: "Command executed successfully".into(),
        error: None,
    }
}

#[tokio::main]
#[tauri::command]
async fn cmd_start_session_websocket_connection(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    session_id: String,
) -> CommandResult {
    let app_state_clone = app_state.clone();
    let ws_thread = app_state_clone.ws_thread.lock().await;

    let machine_token = app_state_clone
        .auth_tokens
        .lock()
        .await
        .machine_auth_token
        .clone();

    let machine_token = match machine_token {
        Some(token) => token,
        None => {
            return CommandResult {
                success: false,
                message: "Machine token not found".into(),
                error: Some(AppError::EmptyToken),
            };
        }
    };

    let mut window_state = app_state_clone.window_state.lock().await;
    let window_clone = window.clone();

    if let Some(join_handle) = ws_thread.as_ref() {
        if !join_handle.inner().is_finished() {
            set_window_application_state(
                &window_clone,
                &WindowApplicationState {
                    route: Some(WindowApplicationRoute::SessionDetails),
                    socket_connected: true,
                    current_session_id: Some(session_id.clone()),
                    current_session_type: None,
                },
                &mut window_state,
            )
            .await
            .unwrap();

            return CommandResult {
                success: false,
                message: "Websocket connection already started".into(),
                error: Some(AppError::WebsocketAlreadyActive),
            };
        }
    }

    drop(ws_thread); // Release the lock

    let window_clone = window.clone();
    let session_id_clone = session_id.clone();
    let window_state_clone = app_state.window_state.clone();
    let ws_handler_clone = app_state.ws_handler.clone();

    let join_handler = tauri::async_runtime::spawn(async move {
        let mut window_state = window_state_clone.lock().await;

        let mut ws_handler = WebsocketHandler::new(
            window_clone.clone(),
            "ws://localhost:8000".to_string(),
            session_id_clone.clone(),
            machine_token,
        );

        ws_handler_clone.lock().await.replace(ws_handler.clone());

        if let Err(e) = ws_handler.run(&mut window_state).await {
            error!("WebSocket connection error: {:?}", e);

            set_window_application_state(
                &window_clone,
                &WindowApplicationState {
                    route: Some(WindowApplicationRoute::SessionDetails),
                    socket_connected: false,
                    current_session_id: Some(session_id_clone.clone()),
                    current_session_type: None,
                },
                &mut window_state,
            )
            .await
            .unwrap();
        }
    });

    app_state.ws_thread.lock().await.replace(join_handler);

    let window_clone = window.clone();
    let session_id_clone = session_id.clone();
    set_window_application_state(
        &window_clone.clone(),
        &WindowApplicationState {
            route: Some(WindowApplicationRoute::SessionDetails),
            socket_connected: true,
            current_session_id: Some(session_id_clone.clone()),
            current_session_type: None,
        },
        &mut window_state,
    )
    .await
    .unwrap();

    return CommandResult {
        success: true,
        message: "Websocket connection started".into(),
        error: None,
    };
}

// In cmd_submituserinput_session_websocket implementation
#[tokio::main]
#[tauri::command]
async fn cmd_submituserinput_session_websocket(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    session_id: String,
    input: String,
) -> CommandResult {
    let ws_handler = app_state.ws_handler.lock().await;

    if let Some(handler) = &*ws_handler {
        let message = serde_json::json!({
            "type": "session",
            "action": "submit",
            "payload": input
        });

        if let Err(e) = handler.send_message(&message).await {
            error!("Failed to send message: {:?}", e);
            return CommandResult {
                success: false,
                message: "Failed to send message".into(),
                error: Some(AppError::WebsocketConnection),
            };
        }

        return CommandResult {
            success: true,
            message: "Message sent successfully".into(),
            error: None,
        };
    }

    CommandResult {
        success: false,
        message: "No active websocket connection".into(),
        error: Some(AppError::WebsocketConnection),
    }
}

#[tauri::command]
async fn cmd_close_splashscreen(window: WebviewWindow) {
    info!("Attempting to close splashscreen");
    match window.get_webview_window("splashscreen") {
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
    match window.get_webview_window("main") {
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

#[tauri::command]
async fn cmd_message_processed(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
    message_id: String,
) -> Result<(), String> {
    debug!("Message processed: {}", message_id);
    let mut window_state = app_state.window_state.lock().await;

    // Here you might want to do something with the processed message ID if needed
    // Process the next message if there is one
    if !window_state.message_queue.is_empty() {
        window::process_next_message(&window, &mut window_state)
            .await
            .unwrap();
    } else {
        window_state.is_processing = false;
    }

    Ok(())
}

#[tauri::command]
async fn cmd_update_remote_sessions(
    window: WebviewWindow,
    app_state: State<'_, ApplicationState>,
) -> Result<CommandResult, String> {
    let api_handler = ApiHandler::new("http://localhost:8000".to_string());
    let mut window_state = app_state.window_state.lock().await;

    // Fetch remote sessions
    let remote_sessions = match api_handler
        .fetch_remote_sessions(
            app_state
                .auth_tokens
                .lock()
                .await
                .user_auth_token
                .clone()
                .unwrap(),
        )
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to fetch remote sessions: {:?}", e);
            return Ok(CommandResult {
                success: false,
                message: "Failed to fetch remote sessions".into(),
                error: Some(AppError::FetchRemoteSessionsFailed),
            });
        }
    };

    // Send remote sessions to the frontend
    send_backend_command(
        &window,
        "update_remote_sessions".into(),
        serde_json::json!(remote_sessions),
        &mut window_state,
    )
    .await
    .unwrap();

    Ok(CommandResult {
        success: true,
        message: "Remote sessions updated successfully".into(),
        error: None,
    })
}

fn main() {
    info!("Starting application");

    let shared_window_state = create_shared_window_state();

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .manage(ApplicationState {
            ws_thread: Arc::new(Mutex::new(None)),
            ws_handler: Arc::new(Mutex::new(None)),
            registration_thread: Arc::new(Mutex::new(None)),
            auth_tokens: Arc::new(Mutex::new(AuthTokens::default())),
            window_state: shared_window_state,
        })
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![
            cmd_start_backend_authentication,
            cmd_register_new_machine,
            cmd_close_splashscreen,
            cmd_message_processed,
            cmd_update_remote_sessions,
            cmd_start_session_websocket_connection,
            cmd_submituserinput_session_websocket
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Application exited");
}
