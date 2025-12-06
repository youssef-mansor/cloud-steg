//! User information structure

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub ip: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_images: Option<Vec<String>>,
    pub status: UserStatus,
    pub registered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    Active,
    Inactive,
    Suspended,
}

impl UserInfo {
    pub fn new(username: impl Into<String>, ip: impl Into<String>, port: u16, sample_images: Option<Vec<String>>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            username: username.into(),
            ip: ip.into(),
            port,
            sample_images,
            status: UserStatus::Active,
            registered_at: now,
            last_seen: now,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn filename(&self) -> String {
        format!("user-{}.json", self.id)
    }
    
    /// Get address as "ip:port" string
    pub fn addr(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.username.is_empty() {
            return Err("Username cannot be empty".to_string());
        }
        if self.ip.is_empty() {
            return Err("IP address cannot be empty".to_string());
        }
        // Basic IP:port validation using SocketAddr
        let addr = format!("{}:{}", self.ip, self.port);
        if addr.parse::<std::net::SocketAddr>().is_err() {
            return Err("Address must be a valid IP:port (e.g., 192.168.1.10:8080)".to_string());
        }
        Ok(())
    }
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserStatus::Active => write!(f, "active"),
            UserStatus::Inactive => write!(f, "inactive"),
            UserStatus::Suspended => write!(f, "suspended"),
        }
    }
}
