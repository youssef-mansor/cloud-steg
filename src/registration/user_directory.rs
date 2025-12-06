//! User Directory implementation using Firebase Storage

use crate::registration::auth::FirebaseAuth;
use crate::registration::config::RegistrationConfig;
use crate::registration::error::RegistrationError;
use crate::registration::user_info::UserInfo;
use cloud_storage::Client;
use futures::stream::StreamExt;
use tracing::{info, warn};

pub struct UserDirectory {
    client: Client,
    config: RegistrationConfig,
}

impl UserDirectory {
    pub async fn new(config: RegistrationConfig) -> Result<Self, RegistrationError> {
        let client = FirebaseAuth::create_client(&config.credentials_path)?;

        info!("UserDirectory initialized with bucket: {}", config.bucket_name);

        Ok(Self { client, config })
    }

    fn get_file_path(&self, filename: &str) -> String {
        format!("{}/{}", self.config.users_folder_prefix, filename)
    }

    pub async fn register_user(&self, user: &UserInfo) -> Result<String, RegistrationError> {
        user.validate()
            .map_err(RegistrationError::ValidationError)?;

        // Check if username already exists
        if self.find_user_by_username(&user.username).await?.is_some() {
            return Err(RegistrationError::UserAlreadyExists(user.username.clone()));
        }

        let file_path = self.get_file_path(&user.filename());
        let json_content = serde_json::to_string_pretty(user)?;

        self.client
            .object()
            .create(
                &self.config.bucket_name,
                json_content.as_bytes().to_vec(),
                &file_path,
                "application/json",
            )
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to register user: {}", e))
            })?;

        info!("Registered user '{}' with ID: {}", user.username, user.id);
        Ok(user.id.clone())
    }

    pub async fn list_users(&self) -> Result<Vec<UserInfo>, RegistrationError> {
        let prefix = format!("{}/", self.config.users_folder_prefix);
        
        let stream = self
            .client
            .object()
            .list(&self.config.bucket_name, Default::default())
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to list users: {}", e))
            })?;

        tokio::pin!(stream);

        let mut users = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(object_list) => {
                    for obj in object_list.items {
                        // Only process files in our users folder
                        if obj.name.starts_with(&prefix) && obj.name.ends_with(".json") {
                            match self.get_user_by_filename(&obj.name).await {
                                Ok(user) => users.push(user),
                                Err(e) => {
                                    warn!("Failed to read user file {}: {}", obj.name, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Error during list operation: {}", e);
                }
            }
        }

        Ok(users)
    }

    async fn get_user_by_filename(&self, filename: &str) -> Result<UserInfo, RegistrationError> {
        let content = self
            .client
            .object()
            .download(&self.config.bucket_name, filename)
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to download user file: {}", e))
            })?;

        let user: UserInfo = serde_json::from_slice(&content)?;
        Ok(user)
    }

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserInfo>, RegistrationError> {
        let all_users = self.list_users().await?;
        Ok(all_users.into_iter().find(|u| u.username == username))
    }

    pub async fn delete_user(&self, user: &UserInfo) -> Result<(), RegistrationError> {
        let file_path = self.get_file_path(&user.filename());

        self.client
            .object()
            .delete(&self.config.bucket_name, &file_path)
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to delete user: {}", e))
            })?;

        info!("Deleted user: {}", user.username);
        Ok(())
    }
}
