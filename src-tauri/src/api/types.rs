// API types and response structures

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserProfile {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
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
    pub status: String,
    pub session_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteSessionsResponse {
    pub sessions: Vec<RemoteSession>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebsocketTokenResponse {
    pub token: String,
}
