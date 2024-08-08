use futures_util::StreamExt;
use log::{debug, error, info, warn};
use serde_json;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, State, Window};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::protocol::Message;

pub mod window;
pub mod ws;

struct ApplicationState {
    auth: Arc<Mutex<Option<ws::AuthState>>>,
    ws_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

#[tokio::main]
#[tauri::command]
async fn start_websocket_connection_command(
    window: Window,
    app_state: State<'_, ApplicationState>,
    auth_session: String,
) -> String {
    info!("Starting websocket connection command");
    let ws_thread = app_state.ws_thread.lock().await;

    match ws_thread.as_ref() {
        Some(join_handle) if !join_handle.inner().is_finished() => {
            warn!("Attempt to start websocket connection when one is already active");
            window::emit_window_message(
                &window,
                window::WindowEventMessage {
                    success: false,
                    message: Default::default(),
                    error: "Websocket connection already started".into(),
                },
            );
            return "Websocket connection already started".into();
        }
        _ => {
            info!("No active websocket connection found. Proceeding to start a new one.");
        }
    }

    drop(ws_thread); // Release the lock

    debug!("Authorizing websocket connection");
    let authorization = ws::authorize_websocket_connection(auth_session).await;

    if authorization.token.is_empty() {
        error!("Failed to authorize websocket connection. Empty token received.");
        window::emit_window_message(
            &window,
            window::WindowEventMessage {
                success: false,
                message: Default::default(),
                error: "Failed to authorize websocket connection".into(),
            },
        );

        return "Failed to authorize websocket connection".into();
    }

    info!("Websocket connection authorized successfully");
    app_state.auth.lock().await.replace(authorization);
    debug!("Emitting authorization success message to window");
    window::emit_window_message(
        &window,
        window::WindowEventMessage {
            success: true,
            message: window::WebsocketMessage {
                message_type: "authorization_success".into(),
                payload: serde_json::Value::Null,
            },
            error: "".into(),
        },
    );

    let auth_state = Arc::clone(&app_state.auth);

    info!("Spawning websocket connection thread");
    let join_handler = tauri::async_runtime::spawn(async move {
        let auth_state = Arc::clone(&auth_state);
        debug!("Starting websocket connection");
        let ws_con = ws::start_websocket_connection(auth_state).await;
        let (_, mut reader) = ws_con.stream.split();

        info!("Websocket connection opened successfully");
        debug!("Emitting connection opened message to window");
        window::emit_window_message(
            &window,
            window::WindowEventMessage {
                success: true,
                message: window::WebsocketMessage {
                    message_type: "connection_opened".into(),
                    payload: serde_json::Value::Null,
                },
                error: "".into(),
            },
        );

        loop {
            debug!("Waiting for next message");
            let message = match reader.next().await {
                Some(msg) => {
                    debug!("Message received: {:?}", msg);
                    msg
                }
                None => {
                    debug!("No message received, waiting for 1 second");
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            match message {
                Ok(Message::Text(msg)) => {
                    debug!("Text message received: {}", msg);
                    let parsed_msg = serde_json::from_str::<window::WebsocketMessage>(&msg);
                    match parsed_msg {
                        Ok(websocket_msg) => {
                            debug!("Emitting received message to window");
                            window::emit_window_message(
                                &window,
                                window::WindowEventMessage {
                                    success: true,
                                    message: websocket_msg,
                                    error: "".into(),
                                },
                            );
                        }
                        Err(e) => {
                            error!("Failed to parse websocket message: {:?}", e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Websocket connection closed");
                    debug!("Emitting connection closed message to window");
                    window::emit_window_message(
                        &window,
                        window::WindowEventMessage {
                            success: true,
                            message: window::WebsocketMessage {
                                message_type: "connection_closed".into(),
                                payload: serde_json::Value::Null,
                            },
                            error: "".into(),
                        },
                    );
                    break;
                }
                Ok(other) => {
                    debug!("Received non-text message: {:?}", other);
                }
                Err(e) => {
                    error!("Error receiving message: {:?}", e);
                }
            }
        }
    });

    app_state.ws_thread.lock().await.replace(join_handler);
    info!("Websocket connection thread spawned and stored");

    "Websocket connection started".into()
}

#[tauri::command]
async fn close_splashscreen(window: Window) {
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

    let app = tauri::Builder::default()
        .manage(ApplicationState {
            auth: Default::default(),
            ws_thread: Default::default(),
        })
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![
            start_websocket_connection_command,
            close_splashscreen
        ]);

    info!("Application builder configured");
    debug!("Running application");

    match app.run(tauri::generate_context!()) {
        Ok(_) => info!("Application exited successfully"),
        Err(e) => error!("Error while running tauri application: {:?}", e),
    }
}
