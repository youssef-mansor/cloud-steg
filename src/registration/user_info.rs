//! User information structure

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub addr: String,
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
    pub fn new(username: impl Into<String>, addr: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            username: username.into(),
            addr: addr.into(),
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
    
    pub fn validate(&self) -> Result<(), String> {
        if self.username.is_empty() {
            return Err("Username cannot be empty".to_string());
        }
        if self.addr.is_empty() {
            return Err("Address cannot be empty".to_string());
        }
        // Basic IP:port validation using SocketAddr
        if self.addr.parse::<std::net::SocketAddr>().is_err() {
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
