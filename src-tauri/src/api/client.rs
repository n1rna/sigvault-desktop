// API client implementation

use log::debug;
use reqwest::Client;
use serde_json;

use crate::error::Result;
use crate::machine::MachineInformation;

use super::types::*;

pub struct ApiClient {
    base_url: String,
    client: Client,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
        }
    }

    pub async fn user_profile(&self, auth_token: String) -> Result<UserProfile> {
        debug!("Fetching user profile");
        let url = format!("{}/api/v2/user/", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {auth_token}"))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    pub async fn fetch_remote_sessions(
        &self,
        auth_token: String,
    ) -> Result<RemoteSessionsResponse> {
        debug!("Fetching remote sessions");
        let url = format!("{}/api/v2/remote-sessions/list", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {auth_token}"))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    pub async fn fetch_websocket_token(
        &self,
        auth_token: String,
        session_id: String,
        machine_info: MachineInformation,
    ) -> Result<WebsocketTokenResponse> {
        debug!("Fetching WebSocket token");
        let url = format!("{}/api/v2/machine/ws-token", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {auth_token}"))
            .json(&serde_json::json!({
                "machine_id": machine_info.machine_id,
                "machine_type": machine_info.machine_type,
                "session_id": session_id,
            }))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }
}
