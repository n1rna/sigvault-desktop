// src/ws_api_handler.rs

use log::debug;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::machine::MachineInformation;

pub struct ApiHandler {
    api_base_url: String,
    client: Client,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthorizeUserResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthorizeMachineResponse {
    pub token: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MachineRegistrationResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteSession {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteSessionsResponse {
    pub sessions: Vec<RemoteSession>,
}


impl ApiHandler {
    pub fn new(api_base_url: String) -> Self {
        Self {
            api_base_url,
            client: Client::new(),
        }
    }

    pub async fn authorize_user_websocket_connection(
        &self,
        auth_token: String,
    ) -> Result<AuthorizeUserResponse, reqwest::Error> {
        debug!("Authorizing user websocket connection");
        let url = format!("{}/api/v1/authorize-user", self.api_base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_token)
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    pub async fn authorize_machine_websocket_connection(
        &self,
        auth_token: String,
        machine_info: &MachineInformation,
    ) -> Result<AuthorizeMachineResponse, reqwest::Error> {
        debug!("Authorizing machine websocket connection");
        let url = format!("{}/api/v1/authorize-machine", self.api_base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&serde_json::json!({"machine_id": machine_info.machine_id}))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    pub async fn register_new_machine(
        &self,
        auth_token: String,
        machine_id: String,
        machine_name: String,
        machine_type: String,
    ) -> Result<MachineRegistrationResponse, reqwest::Error> {
        debug!("Registering new machine");
        let url = format!("{}/api/v1/register-machine", self.api_base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&serde_json::json!({
                "machine_id": machine_id,
                "machine_name": machine_name,
                "machine_type": machine_type,
            }))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }

    pub async fn fetch_remote_sessions(
        &self,
        auth_token: String,
    ) -> Result<RemoteSessionsResponse, reqwest::Error> {
        debug!("Fetching remote sessions");
        let url = format!("{}/api/v1/remote-sessions", self.api_base_url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", auth_token))
            .send()
            .await?
            .json()
            .await?;

        Ok(response)
    }
}
