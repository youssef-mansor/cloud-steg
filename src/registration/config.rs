//! Configuration for user registration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationConfig {
    pub credentials_path: PathBuf,
    pub users_folder_name: String,
    pub shared_drive_id: Option<String>,
}

impl RegistrationConfig {
    pub fn new(
        credentials_path: impl Into<PathBuf>,
        users_folder_name: impl Into<String>,
    ) -> Self {
        Self {
            credentials_path: credentials_path.into(),
            users_folder_name: users_folder_name.into(),
            shared_drive_id: None,
        }
    }

    pub fn with_shared_drive(mut self, drive_id: impl Into<String>) -> Self {
        self.shared_drive_id = Some(drive_id.into());
        self
    }
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            credentials_path: PathBuf::from("credentials/service-account.json"),
            users_folder_name: "registered-users".to_string(),
            shared_drive_id: None,
        }
    }
}
