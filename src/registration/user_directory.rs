//! User Directory implementation using Firebase Storage
//! Structure: users/{username}/profile.json

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

    /// Get the profile path for a user
    fn get_profile_path(&self, username: &str) -> String {
        format!("users/{}/profile.json", username)
    }

    /// Get the user folder prefix
    fn get_user_folder(&self, username: &str) -> String {
        format!("users/{}/", username)
    }

    /// Check if a user exists by trying to download their profile
    async fn user_exists(&self, username: &str) -> Result<bool, RegistrationError> {
        let profile_path = self.get_profile_path(username);
        
        match self.client.object().download(&self.config.bucket_name, &profile_path).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                // If 404 or "No such object", user doesn't exist
                if err_str.contains("404") 
                    || err_str.contains("not found") 
                    || err_str.contains("No such object") {
                    Ok(false)
                } else {
                    // Real error, propagate it
                    Err(RegistrationError::FirebaseApiError(format!("Error checking user existence: {}", e)))
                }
            }
        }
    }

    pub async fn register_user(&self, user: &UserInfo) -> Result<String, RegistrationError> {
        user.validate()
            .map_err(RegistrationError::ValidationError)?;

        // Check if username already exists (try to download profile)
        let profile_path = self.get_profile_path(&user.username);
        
        // Check existence - this now properly handles 404
        match self.user_exists(&user.username).await {
            Ok(true) => {
                return Err(RegistrationError::UserAlreadyExists(user.username.clone()));
            }
            Ok(false) => {
                // User doesn't exist, proceed with registration
            }
            Err(e) => {
                // Real error during check
                return Err(e);
            }
        }

        let json_content = serde_json::to_string_pretty(user)?;

        self.client
            .object()
            .create(
                &self.config.bucket_name,
                json_content.as_bytes().to_vec(),
                &profile_path,
                "application/json",
            )
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to register user: {}", e))
            })?;

        info!("Registered user '{}' at path: {}", user.username, profile_path);
        Ok(user.id.clone())
    }


    pub async fn get_user(&self, username: &str) -> Result<UserInfo, RegistrationError> {
        let profile_path = self.get_profile_path(username);
        
        let content = self
            .client
            .object()
            .download(&self.config.bucket_name, &profile_path)
            .await
            .map_err(|e| {
                if e.to_string().contains("404") || e.to_string().contains("not found") {
                    RegistrationError::UserNotFound(username.to_string())
                } else {
                    RegistrationError::FirebaseApiError(format!("Failed to download user profile: {}", e))
                }
            })?;

        let user: UserInfo = serde_json::from_slice(&content)?;
        Ok(user)
    }

    pub async fn list_users(&self) -> Result<Vec<UserInfo>, RegistrationError> {
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
                        // Only process profile.json files
                        if obj.name.starts_with("users/") && obj.name.ends_with("/profile.json") {
                            match self.get_user_by_path(&obj.name).await {
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

    async fn get_user_by_path(&self, path: &str) -> Result<UserInfo, RegistrationError> {
        let content = self
            .client
            .object()
            .download(&self.config.bucket_name, path)
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
        match self.get_user(username).await {
            Ok(user) => Ok(Some(user)),
            Err(RegistrationError::UserNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn delete_user(&self, username: &str) -> Result<(), RegistrationError> {
        let profile_path = self.get_profile_path(username);

        self.client
            .object()
            .delete(&self.config.bucket_name, &profile_path)
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to delete user: {}", e))
            })?;

        info!("Deleted user: {}", username);
        Ok(())
    }

    /// Get the client for image operations
    pub fn get_client(&self) -> &Client {
        &self.client
    }

    /// Get the bucket name
    pub fn get_bucket_name(&self) -> &str {
        &self.config.bucket_name
    }
}
