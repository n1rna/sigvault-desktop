// WebSocket connection handler

use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use log::{debug, error, info};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tauri::WebviewWindow;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{frame::CloseFrame, Message},
};

use crate::{
    error::Result,
    window::{update_session_state, update_state, SessionEvent, StateUpdateEvent},
};
use crate::{
    websocket::types::WorkflowMessagePayload,
    window::{SessionMessageType, WindowApplicationRoute},
};

use super::types::{GenericWebsocketMessage, InitializeSessionMessage};

type WebSocketStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsSender = SplitSink<WebSocketStream, Message>;
type WsReceiver = SplitStream<WebSocketStream>;

#[derive(Clone)]
pub struct WebsocketHandler {
    window: WebviewWindow,
    ws_base_url: String,
    sender: Arc<Mutex<Option<WsSender>>>,
    session_id: String,
    token: String,
    is_connected: Arc<Mutex<bool>>,
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
            is_connected: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn close(&mut self) -> Result<()> {
        debug!("Initiating websocket connection closure");
        let mut sender_lock = self.sender.lock().await;

        if let Some(sender) = &mut *sender_lock {
            let close_frame = Message::Close(Some(CloseFrame {
                code: 1000u16.into(),
                reason: "Client initiated closure".into(),
            }));

            if let Err(e) = sender.send(close_frame).await {
                error!("Error sending close frame: {:?}", e);
            }

            *sender_lock = None;
            *self.is_connected.lock().await = false;

            debug!("Websocket connection closed successfully");
        }

        Ok(())
    }

    async fn send_initialize_message(&mut self) -> Result<()> {
        let mut sender_lock = self.sender.lock().await;

        if let Some(sender) = &mut *sender_lock {
            let init_message = InitializeSessionMessage::new();
            let message = Message::Text(serde_json::to_string(&init_message)?);
            sender.send(message).await.map_err(|e| {
                crate::error::AppError::WebsocketConnection(format!("Failed to send: {}", e))
            })?;
            debug!("Sent initialization message");
        }

        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        debug!("Starting websocket connection");
        debug!("Token: {}", self.token);

        let url = format!(
            "{}/api/v2/ws/connect/{}?token={}",
            self.ws_base_url, self.session_id, self.token
        );

        let (ws_stream, _) = connect_async(url).await.map_err(|e| {
            crate::error::AppError::WebsocketConnection(format!("Connection failed: {}", e))
        })?;

        let (sender, mut receiver) = ws_stream.split();

        self.sender.lock().await.replace(sender);
        *self.is_connected.lock().await = true;

        info!("Websocket connection opened successfully");
        self.emit_connection_opened().await?;

        loop {
            debug!("Waiting for next message");
            match self.handle_next_message(&mut receiver).await {
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

        self.cleanup().await?;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        debug!("Performing websocket cleanup");

        if *self.is_connected.lock().await {
            if let Err(e) = self.close().await {
                error!("Error during connection closure: {:?}", e);
            }
        }

        *self.sender.lock().await = None;
        self.emit_connection_closed().await?;

        debug!("Websocket cleanup completed");
        Ok(())
    }

    async fn handle_next_message(&mut self, reader: &mut WsReceiver) -> Result<bool> {
        match reader.next().await {
            Some(Ok(Message::Text(msg))) => {
                debug!("Text message received: {}", msg);

                let parsed_msg: std::result::Result<GenericWebsocketMessage, serde_json::Error> =
                    serde_json::from_str(&msg);

                match parsed_msg {
                    Ok(msg) => {
                        match msg.message_type {
                            SessionMessageType::AuthenticationMessage => {
                                if msg.success != true {
                                    error!(
                                        "Authentication failed: {}",
                                        msg.message.unwrap_or_default()
                                    );
                                    return Ok(true);
                                }

                                debug!(
                                    "Authentication success received, sending initialize message"
                                );

                                update_state(
                                    &self.window,
                                    StateUpdateEvent::builder()
                                        .socket_connected(true)
                                        .session_id(self.session_id.clone())
                                        .route(WindowApplicationRoute::SessionDetails)
                                        .build(),
                                )
                                .await
                                .map_err(|e| {
                                    crate::error::AppError::WindowError(format!(
                                        "Failed to set state: {}",
                                        e
                                    ))
                                })?;

                                // Send the initialize message
                                if let Err(e) = self.send_initialize_message().await {
                                    error!("Failed to send initialize message: {:?}", e);
                                    return Ok(true);
                                }
                            }
                            SessionMessageType::SystemMessage => {
                                debug!("System message received: {:?}", msg.payload);
                                return Ok(false);
                            }
                            SessionMessageType::WorkflowMessage => {
                                debug!("Workflow message received");
                                // use workflow handler to process the workflow message received:
                                // eventually we will update the state based on the workflow message
                                match msg.payload {
                                    Some(payload) => {
                                        self.handle_workflow_message(payload).await?;
                                    }
                                    None => {
                                        if msg.success == false {
                                            error!(
                                                "Workflow message error: {}",
                                                msg.message.unwrap_or_default()
                                            );
                                        } else {
                                            debug!("Workflow message received with no payload");
                                        }
                                    }
                                }
                            }
                        }

                        Ok(false)
                    }
                    Err(e) => {
                        error!("Error parsing message: {:?}", e);
                        Ok(false)
                    }
                }
            }
            Some(Ok(Message::Close(_))) => {
                info!("Websocket connection closed");
                self.emit_connection_closed().await?;
                Ok(true)
            }
            Some(Ok(other)) => {
                debug!("Received non-text message: {:?}", other);
                Ok(false)
            }
            Some(Err(e)) => Err(crate::error::AppError::WebsocketConnection(format!(
                "Stream error: {}",
                e
            ))),
            None => {
                debug!("No message received, waiting for 1 second");
                sleep(Duration::from_secs(1)).await;
                Ok(false)
            }
        }
    }

    async fn handle_workflow_message(&mut self, payload: WorkflowMessagePayload) -> Result<()> {
        debug!("Handling workflow message: {:?}", payload);
        // Process the workflow message here
        update_session_state(
            &self.window,
            SessionEvent::builder()
                .requirements(payload.requirements)
                .data(payload.data)
                .step(payload.step)
                .session_type(payload.session_type)
                .finished(payload.finished)
                .success(payload.success)
                .message(payload.message.unwrap_or_default())
                .build(),
        )
        .await
        .map_err(|e: Box<dyn Error + Send + 'static>| {
            crate::error::AppError::WindowError(format!("Failed to set state: {}", e))
        })?;

        if payload.finished {
            debug!("Workflow step finished, updating application state");
            self.close().await?;

            update_state(
                &self.window,
                StateUpdateEvent::builder()
                    .route(WindowApplicationRoute::RemoteSessions)
                    .build(),
            )
            .await
            .map_err(|e| {
                crate::error::AppError::WindowError(format!("Failed to set state: {}", e))
            })?;
        }

        Ok(())
    }

    pub async fn send_message(&self, payload: &serde_json::Value) -> Result<()> {
        if let Some(sender) = &mut *self.sender.lock().await {
            let message = Message::Text(serde_json::to_string(payload)?);
            sender.send(message).await.map_err(|e| {
                crate::error::AppError::WebsocketConnection(format!("Failed to send: {}", e))
            })?;
            debug!("Message sent successfully: {:?}", payload);
            Ok(())
        } else {
            error!("No active websocket connection");
            Err(crate::error::AppError::WebsocketConnection(
                "No active connection".to_string(),
            ))
        }
    }

    async fn emit_connection_opened(&self) -> Result<()> {
        debug!("Emitting connection opened message to window");

        update_state(
            &self.window,
            StateUpdateEvent::builder()
                .socket_connected(true)
                .session_id(self.session_id.clone())
                .build(),
        )
        .await
        .map_err(|e| crate::error::AppError::WindowError(format!("Failed to set state: {}", e)))?;

        debug!("Connection opened message emitted");
        Ok(())
    }

    async fn emit_connection_closed(&self) -> Result<()> {
        debug!("Emitting connection closed message to window");
        update_state(
            &self.window,
            StateUpdateEvent::builder()
                .socket_connected(false)
                .route(WindowApplicationRoute::RemoteSessions)
                .build(),
        )
        .await
        .map_err(|e| crate::error::AppError::WindowError(format!("Failed to set state: {}", e)))?;

        Ok(())
    }
}
