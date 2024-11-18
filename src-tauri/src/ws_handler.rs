// src/ws_handler.rs

use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt,
};
use futures_util::StreamExt;
use log::{debug, error, info};
use serde_json;
use std::sync::Arc;
use std::time::Duration;
use tauri::WebviewWindow;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::window::{
    emit_window_message, set_window_application_state, BackendEventMessage, MessageType,
    SessionMessageType, WindowApplicationRoute, WindowApplicationState, WindowEventMessage,
    WindowState,
};

type WebSocketStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsSender = SplitSink<WebSocketStream, Message>;
type WsReceiver = SplitStream<WebSocketStream>;

#[derive(Clone)]
pub struct WebsocketHandler {
    window: WebviewWindow,
    ws_base_url: String,
    sender: Arc<tokio::sync::Mutex<Option<WsSender>>>, // Changed to tokio::sync::Mutex
    session_id: String,
    token: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitializeSessionMessage {
    r#type: String,
    action: String,
    payload: serde_json::Value,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GenericWebsocketMessage {
    message_type: SessionMessageType,
    payload: serde_json::Value,
}

impl WebsocketHandler {
    pub fn new(
        window: WebviewWindow,
        ws_base_url: String,
        session_id: String,
        token: String,
    ) -> Self {
        Self {
            window,
            ws_base_url,
            sender: Arc::new(Mutex::new(None)),
            session_id,
            token,
        }
    }

    async fn send_initialize_message(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let mut sender_lock = self.sender.lock().await;

        if let Some(sender) = &mut *sender_lock {
            let init_message = InitializeSessionMessage {
                r#type: "session".to_string(),
                action: "initialize".to_string(),
                payload: serde_json::json!({}),
            };

            let message = Message::Text(serde_json::to_string(&init_message).unwrap());
            sender.send(message).await.unwrap();
            debug!("Sent initialization message");
        }
        Ok(())
    }

    pub async fn run(
        &mut self,
        window_state: &mut WindowState,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        debug!("Starting websocket connection");
        debug!("Token: {}", self.token);
        let url = format!(
            "{}/api/v1/{}?token={}",
            self.ws_base_url, self.session_id, self.token
        );
        let (ws_stream, _) = connect_async(url).await.unwrap();
        let (sender, mut receiver) = ws_stream.split();

        self.sender.lock().await.replace(sender);

        info!("Websocket connection opened successfully");
        self.emit_connection_opened(window_state).await;

        loop {
            debug!("Waiting for next message");
            match self.handle_next_message(&mut receiver, window_state).await {
                Ok(should_break) => {
                    if should_break {
                        break;
                    }
                }
                Err(e) => {
                    error!("Error handling message: {:?}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_next_message<T>(
        &mut self,
        reader: &mut T,
        window_state: &mut WindowState,
    ) -> Result<bool, Box<dyn std::error::Error>>
    where
        T: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        match reader.next().await {
            Some(Ok(Message::Text(msg))) => {
                debug!("Text message received: {}", msg);

                let parsed_msg: Result<GenericWebsocketMessage, serde_json::Error> =
                    serde_json::from_str(&msg);

                match parsed_msg {
                    Ok(msg) => {
                        match msg.message_type {
                            SessionMessageType::AuthorizationSuccess => {
                                debug!(
                                    "Authorization success received, sending initialize message"
                                );
                                set_window_application_state(
                                    &self.window,
                                    &WindowApplicationState {
                                        route: None,
                                        socket_connected: true,
                                        current_session_id: Some(self.session_id.clone()),
                                        current_session_type: Some(
                                            msg.payload["session_type"]
                                                .as_str()
                                                .unwrap()
                                                .to_string(),
                                        ),
                                    },
                                    window_state,
                                )
                                .await
                                .unwrap();

                                if let Err(e) = self.send_initialize_message().await {
                                    error!("Failed to send initialize message: {:?}", e);
                                }
                                return Ok(false);
                            }
                            _ => {
                                debug!("Emitting session message to window");
                                self.emit_session_message(serde_json::to_value(msg)?, window_state)
                                    .await;
                            }
                        };

                        return Ok(false);
                    }
                    Err(e) => {
                        error!("Error parsing message: {:?}", e);
                        return Ok(false);
                    }
                }
            }
            Some(Ok(Message::Close(_))) => {
                info!("Websocket connection closed");
                self.emit_connection_closed(window_state).await;
                Ok(true)
            }
            Some(Ok(other)) => {
                debug!("Received non-text message: {:?}", other);
                Ok(false)
            }
            Some(Err(e)) => Err(Box::new(e)),
            None => {
                debug!("No message received, waiting for 1 second");
                sleep(Duration::from_secs(1)).await;
                Ok(false)
            }
        }
    }

    pub async fn send_message(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        if let Some(sender) = &mut *self.sender.lock().await {
            let message = Message::Text(serde_json::to_string(payload).unwrap());
            sender.send(message).await.unwrap();
            debug!("Message sent successfully: {:?}", payload);
            Ok(())
        } else {
            error!("No active websocket connection");
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "No active websocket connection",
            )))
        }
    }

    async fn emit_connection_opened(&self, window_state: &mut WindowState) {
        debug!("Emitting connection opened message to window");
        set_window_application_state(
            &self.window,
            &WindowApplicationState {
                route: None,
                socket_connected: true,
                current_session_id: Some(self.session_id.clone()),
                current_session_type: None,
            },
            window_state,
        )
        .await
        .unwrap();
        debug!("Connection opened message emitted");
    }

    async fn emit_connection_closed(&self, window_state: &mut WindowState) {
        debug!("Emitting connection closed message to window");
        set_window_application_state(
            &self.window,
            &WindowApplicationState {
                route: Some(WindowApplicationRoute::RemoteSessions),
                socket_connected: false,
                current_session_id: None,
                current_session_type: None,
            },
            window_state,
        )
        .await
        .unwrap();
    }

    async fn emit_session_message(
        &self,
        payload: serde_json::Value,
        window_state: &mut WindowState,
    ) {
        emit_window_message(
            &self.window,
            WindowEventMessage {
                success: true,
                message: BackendEventMessage {
                    message_type: MessageType::SessionMessage,
                    payload,
                },
                error: None,
            },
            window_state,
        )
        .await
        .unwrap();
    }
}
