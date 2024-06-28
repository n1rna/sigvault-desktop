use futures_util::{SinkExt, StreamExt};
use std::sync::Mutex;
use tauri::{State, Window};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(serde::Deserialize)]
struct AuthState {
    token: String,
    session_id: String,
}

async fn authorize_websocket_connection(auth_token: String) -> AuthState {
    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:8000/api/v1/authorize")
        .header("Authorization", auth_token)
        .send()
        .await
        .unwrap();

    let body = res.text().await.unwrap();
    print!("Response: {}", body);
    let auth_state: AuthState = serde_json::from_str(&body).unwrap();
    auth_state
}

async fn start_websocket_connection(ws_session_id: String, ws_token: String) {
    let (ws_stream, _) = connect_async(format!(
        "ws://localhost:8000/api/v1/{}?token={}",
        ws_session_id, ws_token
    ))
    .await
    .expect("Failed to connect to websocket server");

    let (mut write, read) = ws_stream.split();

    tokio::spawn(async move {
        read.for_each(|message| async {
            match message {
                Ok(msg) => {
                    // Handle received message
                    println!("Received message: {:?}", msg);
                }
                Err(e) => {
                    // Handle error
                    eprintln!("Error receiving message: {:?}", e);
                }
            }
        })
        .await;
    });

    // Send a message to the server
    let message = Message::Text("Hello from Rust!".to_string());
    write.send(message).await.expect("Failed to send message");
}

#[tauri::command]
fn start_websocket_connection_command(
    window: Window,
    auth_state: State<Mutex<AuthState>>,
    auth_session: String,
) -> String {
    println!(
        "Starting websocket connection with session: {}",
        auth_session
    );
    // perform authorization first based on the auth token (message)
    let authorization = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(authorize_websocket_connection(auth_session));

    // Update the auth state with the token
    *auth_state.lock().unwrap() = authorization;

    window
        .emit(
            "websocket_connection_established",
            Some(r#"{ "token": "fetched" }"#.to_string()),
        )
        .expect("failed to emit event");

    let ws_token = auth_state.lock().unwrap().token.clone();
    let ws_session_id = auth_state.lock().unwrap().session_id.clone();

    tokio::spawn(async move {
        start_websocket_connection(ws_session_id, ws_token).await;
        window
            .emit(
                "websocket_connection_established",
                Some(r#"{ "success": "true" }"#.to_string()),
            )
            .expect("failed to emit event");
    });

    // You can perform any additional logic here, such as updating the UI or handling the result of the command.
    // For example, you can show a success message to the user:
    "fofofof".into()
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AuthState {
            token: "".into(),
            session_id: "".into(),
        }))
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![start_websocket_connection_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
