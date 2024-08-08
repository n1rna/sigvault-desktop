use futures_util::StreamExt;
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
    match app_state.ws_thread.lock().await.as_ref() {
        Some(join_handle) => {
            if !join_handle.inner().is_finished() {
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
        }
        None => {
            return "Websocket connection could not be started".into();
        }
    }

    let authorization = ws::authorize_websocket_connection(auth_session).await;

    if authorization.token.is_empty() {
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

    app_state.auth.lock().await.replace(authorization);
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

    let join_handler = tauri::async_runtime::spawn(async move {
        let auth_state = Arc::clone(&auth_state);
        let ws_con = ws::start_websocket_connection(auth_state).await;
        let (_, mut reader) = ws_con.stream.split();

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
            let message = match reader.next().await {
                Some(msg) => {
                    println!("MESSAGE RECEIVED {:?}", msg);
                    msg
                }
                None => {
                    println!("No message received");
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            match message {
                Ok(Message::Text(msg)) => {
                    window::emit_window_message(
                        &window,
                        window::WindowEventMessage {
                            success: true,
                            message: serde_json::from_str::<window::WebsocketMessage>(&msg)
                                .unwrap(),
                            error: "".into(),
                        },
                    );
                }
                Ok(Message::Close(_)) => {
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
                _ => {}
            }
        }
    });

    app_state.ws_thread.lock().await.replace(join_handler);

    "Websocket connection started".into()
}

#[tauri::command]
async fn close_splashscreen(window: Window) {
    // Close splashscreen
    match window.get_window("splashscreen") {
        Some(splashscreen) => {
            splashscreen.close().unwrap();
        }
        None => {
            println!("No splashscreen window found");
        }
    }

    // Show main window
    window
        .get_window("main")
        .expect("no window labeled 'main' found")
        .show()
        .unwrap();
}

fn main() {
    tauri::Builder::default()
        // .manage(AuthStateMutex::default())
        // .manage(WSConnectionMutex::default())
        .manage(ApplicationState {
            auth: Default::default(),
            ws_thread: Default::default(),
        })
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![
            start_websocket_connection_command,
            close_splashscreen
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
