// src/ws_handler.rs

use futures_util::StreamExt;
use log::{debug, error, info};
use serde_json;
use std::time::Duration;
use tauri::Window;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::window::{emit_window_message, set_window_application_state, BackendEventMessage, MessageType, WindowApplicationState, WindowEventMessage, WindowApplicationRoute};

pub struct WebsocketHandler {
    window: Window,
    ws_base_url: String,
}

impl WebsocketHandler {
    pub fn new(window: Window, ws_base_url: String) -> Self {
        Self { window, ws_base_url }
    }

    pub async fn run(&self, token: String, session_id: String) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Starting websocket connection");
        debug!("Token: {}", token);
        let url = format!("{}/api/v1/{}?token={}", self.ws_base_url, session_id, token);
        let (ws_stream, _) = connect_async(url).await?;
        let (_, mut reader) = ws_stream.split();

        info!("Websocket connection opened successfully");
        self.emit_connection_opened();

        loop {
            debug!("Waiting for next message");
            match self.handle_next_message(&mut reader).await {
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

    async fn handle_next_message<T>(&self, reader: &mut T) -> Result<bool, Box<dyn std::error::Error>>
    where
        T: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    {
        match reader.next().await {
            Some(Ok(Message::Text(msg))) => {
                debug!("Text message received: {}", msg);
                let parsed_msg: serde_json::Value = serde_json::from_str(&msg)?;
                self.emit_text_message(parsed_msg);
                Ok(false)
            }
            Some(Ok(Message::Close(_))) => {
                info!("Websocket connection closed");
                self.emit_connection_closed();
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

    fn emit_connection_opened(&self) {
        debug!("Emitting connection opened message to window");
        set_window_application_state(&self.window, &WindowApplicationState{
            route: WindowApplicationRoute::MainPage,
            socket_connected: true,
        });
    }

    fn emit_connection_closed(&self) {
        debug!("Emitting connection closed message to window");
        set_window_application_state(&self.window, &WindowApplicationState{
            route: WindowApplicationRoute::MainPage,
            socket_connected: false,
        });
    }

    fn emit_text_message(&self, payload: serde_json::Value) {
        emit_window_message(
            &self.window,
            WindowEventMessage {
                success: true,
                message: BackendEventMessage {
                    message_type: MessageType::TextMessage,
                    payload,
                },
                error: None,
            },
        );
    }
}