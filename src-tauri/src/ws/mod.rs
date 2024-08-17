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

#[derive(Debug, serde::Deserialize, Clone)]
pub struct AuthorizerUserResponse {
    pub token: String,
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct AuthorizeMachineResponse {
    pub token: String,
    pub session_id: String,
    pub status: String,
}

#[derive(Debug)]
pub struct WSConnectionState {
    pub stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

pub async fn authorize_user_websocket_connection(auth_token: String) -> AuthorizerUserResponse {
    let client = reqwest::Client::new();
    return match client
        .post("http://localhost:8000/api/v1/authorize-user")
        .header("Authorization", auth_token)
        .send()
        .await
    {
        Ok(res) => {
            let body = res.text().await.unwrap();
            let auth_user_resp = serde_json::from_str::<AuthorizerUserResponse>(&body);
            match auth_user_resp {
                Ok(resp) => resp,
                Err(e) => {
                    println!("Error deserializing AuthState: {:?}", e);
                    AuthorizerUserResponse {
                        token: "".into(),
                    }
                }
            }
        }
        Err(e) => {
            println!("Error: {:?}", e);
            return AuthorizerUserResponse {
                token: "".into(),
            };
        }
    };
}


pub async fn authorize_machine_websocket_connection(auth_token: String, machine_id: String) -> AuthorizeMachineResponse {
    let client = reqwest::Client::new();
    return match client
        .post("http://localhost:8000/api/v1/authorize-machine")
        .header("Authorization", format!("Bearer {}", auth_token))
        .json(&serde_json::json!({"machine_id": machine_id}))
        .send()
        .await
    {
        Ok(res) => {
            let body = res.text().await.unwrap();
            let auth_user_resp = serde_json::from_str::<AuthorizeMachineResponse>(&body);
            match auth_user_resp {
                Ok(resp) => resp,
                Err(e) => {
                    println!("Error deserializing AuthState: {:?}", e);
                    AuthorizeMachineResponse {
                        token: "".into(),
                        session_id: "".into(),
                        status: "".into(),
                    }
                }
            }
        }
        Err(e) => {
            println!("Error: {:?}", e);
            return AuthorizeMachineResponse {
                token: "".into(),
                session_id: "".into(),
                status: "".into(),
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
