use serde_json;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[derive(Debug, serde::Deserialize, Clone)]
pub struct AuthState {
    pub token: String,
    pub session_id: String,
}

#[derive(Debug)]
pub struct WSConnectionState {
    pub stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

pub async fn authorize_websocket_connection(auth_token: String) -> AuthState {
    let client = reqwest::Client::new();
    return match client
        .post("http://localhost:8000/api/v1/authorize")
        .header("Authorization", auth_token)
        .send()
        .await
    {
        Ok(res) => {
            let body = res.text().await.unwrap();
            let auth_state = serde_json::from_str::<AuthState>(&body);
            match auth_state {
                Ok(state) => state,
                Err(e) => {
                    println!("Error deserializing AuthState: {:?}", e);
                    AuthState {
                        token: "".into(),
                        session_id: "".into(),
                    }
                }
            }
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

pub async fn start_websocket_connection(auth: Arc<Mutex<Option<AuthState>>>) -> WSConnectionState {
    let auth = auth.lock().await.as_ref().unwrap().clone();
    let (ws_stream, _) = connect_async(format!(
        "ws://localhost:8000/api/v1/{}?token={}",
        auth.session_id, auth.token
    ))
    .await
    .expect("Failed to connect to websocket server");

    WSConnectionState { stream: ws_stream }
}
