//! Configuration for Firebase Storage user registration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationConfig {
    /// Path to the service account credentials JSON file
    pub credentials_path: PathBuf,
    /// Firebase Storage bucket name (e.g., your-project.appspot.com)
    pub bucket_name: String,
    /// Folder/prefix for user files in the bucket
    pub users_folder_prefix: String,
}

impl RegistrationConfig {
    pub fn new(
        credentials_path: impl Into<PathBuf>,
        bucket_name: impl Into<String>,
        users_folder_prefix: impl Into<String>,
    ) -> Self {
        Self {
            credentials_path: credentials_path.into(),
            bucket_name: bucket_name.into(),
            users_folder_prefix: users_folder_prefix.into(),
        }
    }
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            credentials_path: PathBuf::from("credentials/firebase-storage.json"),
            bucket_name: "your-project.appspot.com".to_string(),
            users_folder_prefix: "registered-users".to_string(),
        }
    }
}
