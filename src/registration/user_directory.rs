//! User directory implementation using Google Drive

use crate::registration::auth::{AuthenticatedHub, DriveAuth};
use crate::registration::config::RegistrationConfig;
use crate::registration::error::RegistrationError;
use crate::registration::user_info::UserInfo;
use google_drive3::api::File;
use http_body_util::BodyExt;
use std::io::Cursor;
use tracing::{debug, info, warn};

pub struct UserDirectory {
    hub: AuthenticatedHub,
    config: RegistrationConfig,
    folder_id: Option<String>,
}

impl UserDirectory {
    pub async fn new(config: RegistrationConfig) -> Result<Self, RegistrationError> {
        let hub = DriveAuth::create_hub(&config.credentials_path).await?;

        let mut directory = Self {
            hub,
            config,
            folder_id: None,
        };

        directory.init_folder().await?;
        Ok(directory)
    }

    fn is_shared_drive(&self) -> bool {
        self.config.shared_drive_id.is_some()
    }

    async fn init_folder(&mut self) -> Result<(), RegistrationError> {
        let query = format!(
            "name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
            self.config.users_folder_name
        );

        let mut request = self.hub.files().list().q(&query).param("fields", "files(id, name)");

        if let Some(ref drive_id) = self.config.shared_drive_id {
            request = request
                .corpora("drive")
                .drive_id(drive_id)
                .include_items_from_all_drives(true)
                .supports_all_drives(true);
        }

        let result = request.doit().await.map_err(|e| {
            RegistrationError::DriveApiError(format!("Failed to search for folder: {}", e))
        })?;

        if let Some(files) = result.1.files {
            if let Some(folder) = files.first() {
                self.folder_id = folder.id.clone();
                info!("Found existing users folder: {}", self.config.users_folder_name);
                return Ok(());
            }
        }

        self.create_folder().await
    }

    async fn create_folder(&mut self) -> Result<(), RegistrationError> {
        let mut folder_metadata = File::default();
        folder_metadata.name = Some(self.config.users_folder_name.clone());
        folder_metadata.mime_type = Some("application/vnd.google-apps.folder".to_string());

        if let Some(ref drive_id) = self.config.shared_drive_id {
            folder_metadata.parents = Some(vec![drive_id.clone()]);
        }

        let mut request = self
            .hub
            .files()
            .create(folder_metadata)
            .param("fields", "id, name");

        if self.is_shared_drive() {
            request = request.supports_all_drives(true);
        }

        let result = request
            .upload(Cursor::new(Vec::new()), "application/octet-stream".parse().unwrap())
            .await
            .map_err(|e| {
                RegistrationError::DriveApiError(format!("Failed to create folder: {}", e))
            })?;

        self.folder_id = result.1.id;
        info!("Created users folder: {}", self.config.users_folder_name);
        Ok(())
    }

    fn get_folder_id(&self) -> Result<&str, RegistrationError> {
        self.folder_id
            .as_deref()
            .ok_or_else(|| RegistrationError::FolderNotFound("Users folder not initialized".to_string()))
    }

    pub async fn register_user(&self, user: &UserInfo) -> Result<String, RegistrationError> {
        user.validate().map_err(RegistrationError::ValidationError)?;
        
        // Check if username already exists
        if self.find_user_by_username(&user.username).await?.is_some() {
            return Err(RegistrationError::UserAlreadyExists(user.username.clone()));
        }

        let folder_id = self.get_folder_id()?;
        let json_content = serde_json::to_string_pretty(user)?;

        let mut file_metadata = File::default();
        file_metadata.name = Some(user.filename());
        file_metadata.mime_type = Some("application/json".to_string());
        file_metadata.parents = Some(vec![folder_id.to_string()]);

        let cursor = Cursor::new(json_content.into_bytes());
        let mut request = self
            .hub
            .files()
            .create(file_metadata)
            .param("fields", "id, name");

        if self.is_shared_drive() {
            request = request.supports_all_drives(true);
        }

        let result = request
            .upload(cursor, "application/json".parse().unwrap())
            .await
            .map_err(|e| {
                RegistrationError::DriveApiError(format!("Failed to register user: {}", e))
            })?;

        let file_id = result
            .1
            .id
            .ok_or_else(|| RegistrationError::DriveApiError("No file ID returned".to_string()))?;

        info!("Registered user '{}' with ID: {}", user.username, user.id);
        Ok(file_id)
    }

    pub async fn list_users(&self) -> Result<Vec<UserInfo>, RegistrationError> {
        let folder_id = self.get_folder_id()?;

        let query = format!(
            "'{}' in parents and mimeType = 'application/json' and trashed = false",
            folder_id
        );

        let mut request = self.hub.files().list().q(&query).param("fields", "files(id, name)");

        if let Some(ref drive_id) = self.config.shared_drive_id {
            request = request
                .corpora("drive")
                .drive_id(drive_id)
                .include_items_from_all_drives(true)
                .supports_all_drives(true);
        }

        let result = request.doit().await.map_err(|e| {
            RegistrationError::DriveApiError(format!("Failed to list users: {}", e))
        })?;

        let mut users = Vec::new();

        if let Some(files) = result.1.files {
            for file in files {
                if let Some(ref file_id) = file.id {
                    match self.get_user_by_file_id(file_id).await {
                        Ok(user) => users.push(user),
                        Err(e) => {
                            warn!("Failed to read user file: {}", e);
                        }
                    }
                }
            }
        }

        Ok(users)
    }

    async fn get_user_by_file_id(&self, file_id: &str) -> Result<UserInfo, RegistrationError> {
        let mut request = self.hub.files().get(file_id).param("alt", "media");

        if self.is_shared_drive() {
            request = request.supports_all_drives(true);
        }

        let response = request.doit().await.map_err(|e| {
            RegistrationError::DriveApiError(format!("Failed to get user file: {}", e))
        })?;

        let body_bytes = response
            .0
            .into_body()
            .collect()
            .await
            .map_err(|e| RegistrationError::DriveApiError(format!("Failed to read response: {}", e)))?
            .to_bytes();

        let user: UserInfo = serde_json::from_slice(&body_bytes)?;
        Ok(user)
    }

    pub async fn find_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserInfo>, RegistrationError> {
        let all_users = self.list_users().await?;
        Ok(all_users.into_iter().find(|u| u.username == username))
    }
}
