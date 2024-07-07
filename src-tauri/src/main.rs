use futures::FutureExt;
use futures_util::{stream::SplitSink, stream::SplitStream, StreamExt};
use serde_json;
use std::time::Duration;
use std::{borrow::Borrow, sync::Arc};
use tauri::{State, Window};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};

#[derive(Debug, serde::Deserialize, Clone)]
struct AuthState {
    token: String,
    session_id: String,
}

#[derive(Debug)]
struct WSConnectionState {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct WindowEventMessage {
    success: bool,
    message: WebsocketMessage,
    error: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Default)]
struct WebsocketMessage {
    message_type: String,
    payload: serde_json::Value,
}

struct ApplicationState {
    auth: Arc<Mutex<Option<AuthState>>>,
    ws_thread: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}

async fn authorize_websocket_connection(auth_token: String) -> AuthState {
    let client = reqwest::Client::new();
    return match client
        .post("http://localhost:8000/api/v1/authorize")
        .header("Authorization", auth_token)
        .send()
        .await
    {
        Ok(res) => {
            let body = res.text().await.unwrap();
            let auth_state = serde_json::from_str::<AuthState>(&body).unwrap();
            auth_state
        }
        Err(e) => {
            println!("Error: {:?}", e);
            return AuthState {
                token: "".into(),
                session_id: "".into(),
            };
        }
    };
}

async fn start_websocket_connection(auth: Arc<Mutex<Option<AuthState>>>) -> WSConnectionState {
    let auth = auth.lock().await.as_ref().unwrap().clone();
    let (ws_stream, _) = connect_async(format!(
        "ws://localhost:8000/api/v1/{}?token={}",
        auth.session_id, auth.token
    ))
    .await
    .expect("Failed to connect to websocket server");

    WSConnectionState { stream: ws_stream }
}

fn emit_window_message(window: &Window, message: WindowEventMessage) {
    window
        .emit(
            "websocket_connection",
            serde_json::to_string(&message).unwrap(),
        )
        .expect("failed to emit event");
}

#[tokio::main]
#[tauri::command]
async fn start_websocket_connection_command(
    window: Window,
    app_state: State<'_, ApplicationState>,
    auth_session: String,
) -> String {
    if app_state.ws_thread.lock().await.is_some() {
        emit_window_message(
            &window,
            WindowEventMessage {
                success: false,
                message: Default::default(),
                error: "Websocket connection already started".into(),
            },
        );

        return "Websocket connection already started".into();
    }

    let authorization = authorize_websocket_connection(auth_session).await;
    if authorization.token.is_empty() {
        emit_window_message(
            &window,
            WindowEventMessage {
                success: false,
                message: Default::default(),
                error: "Failed to authorize websocket connection".into(),
            },
        );

        return "Failed to authorize websocket connection".into();
    }

    app_state.auth.lock().await.replace(authorization);
    emit_window_message(
        &window,
        WindowEventMessage {
            success: true,
            message: WebsocketMessage {
                message_type: "authorization_success".into(),
                payload: serde_json::Value::Null,
            },
            error: "".into(),
        },
    );

    let auth_state = Arc::clone(&app_state.auth);

    let join_handler = tauri::async_runtime::spawn(async move {
        let auth_state = Arc::clone(&auth_state);
        let ws_con = start_websocket_connection(auth_state).await;
        let (_, mut reader) = ws_con.stream.split();

        emit_window_message(
            &window,
            WindowEventMessage {
                success: true,
                message: WebsocketMessage {
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
                    emit_window_message(
                        &window,
                        WindowEventMessage {
                            success: true,
                            message: serde_json::from_str::<WebsocketMessage>(&msg).unwrap(),
                            error: "".into(),
                        },
                    );
                }
                Ok(Message::Close(_)) => {
                    emit_window_message(
                        &window,
                        WindowEventMessage {
                            success: true,
                            message: WebsocketMessage {
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

fn main() {
    tauri::Builder::default()
        // .manage(AuthStateMutex::default())
        // .manage(WSConnectionMutex::default())
        .manage(ApplicationState {
            auth: Default::default(),
            ws_thread: Default::default(),
        })
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![start_websocket_connection_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
