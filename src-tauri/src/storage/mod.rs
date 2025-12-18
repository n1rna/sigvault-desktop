// Secure storage using Tauri Stronghold
// TODO: Integrate with Stronghold once API is properly configured

use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

const STORE_FILENAME: &str = "auth.dat";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredAuthData {
    pub oauth_access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

pub struct SecureStorage {
    app: AppHandle,
    _initialized: Arc<Mutex<bool>>,
}

impl SecureStorage {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            _initialized: Arc::new(Mutex::new(false)),
        }
    }

    fn get_storage_path(&self) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let app_data_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {}", e))?;

        // Create directory if it doesn't exist
        fs::create_dir_all(&app_data_dir)?;

        Ok(app_data_dir.join(STORE_FILENAME))
    }

    /// Initialize the stronghold with a password
    pub async fn initialize(&self, _password: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Initializing secure storage");
        // TODO: Initialize stronghold when API is available
        Ok(())
    }

    /// Store authentication data securely
    pub async fn store_auth_data(
        &self,
        auth_data: &StoredAuthData,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Storing auth data securely");

        // Serialize auth data
        let json = serde_json::to_string_pretty(auth_data)?;

        // TODO: Use Stronghold for encryption when API is configured
        // For now, use filesystem storage
        let path = self.get_storage_path()?;
        fs::write(&path, json)?;

        debug!("Auth data stored successfully at: {:?}", path);
        Ok(())
    }

    /// Retrieve authentication data
    pub async fn get_auth_data(&self) -> Result<StoredAuthData, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Retrieving auth data");

        // TODO: Use Stronghold for decryption when API is configured
        let path = self.get_storage_path()?;

        if !path.exists() {
            debug!("No auth data found");
            return Ok(StoredAuthData::default());
        }

        let json = fs::read_to_string(&path)?;
        let auth_data: StoredAuthData = serde_json::from_str(&json)?;

        debug!("Auth data retrieved successfully");
        Ok(auth_data)
    }

    /// Clear all stored authentication data
    pub async fn clear_auth_data(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Clearing auth data");

        // TODO: Use Stronghold when API is configured
        let path = self.get_storage_path()?;

        if path.exists() {
            fs::remove_file(&path)?;
        }

        debug!("Auth data cleared");
        Ok(())
    }

    /// Update OAuth access token
    pub async fn update_oauth_access_token(
        &self,
        token: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut data = self.get_auth_data().await.unwrap_or_default();
        data.oauth_access_token = Some(token);
        self.store_auth_data(&data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_auth_data_serialization() {
        let data = StoredAuthData {
            oauth_access_token: Some("oauth_token".to_string()),
            refresh_token: None,
            expires_at: Some(1234567890),
        };

        let serialized = serde_json::to_string(&data).unwrap();
        let deserialized: StoredAuthData = serde_json::from_str(&serialized).unwrap();

        assert_eq!(data.oauth_access_token, deserialized.oauth_access_token);
    }
}
