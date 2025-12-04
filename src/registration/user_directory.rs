use crate::registration::auth::{AuthenticatedHub, DriveAuth};
use crate::registration::config::RegistrationConfig;
use crate::registration::error::RegistrationError;
use crate::registration::user_info::UserInfo;
use google_drive3::api::File;
use http_body_util::BodyExt;
use std::io::Cursor;
use tracing::{info, warn};

pub struct UserDirectory {
    hub: AuthenticatedHub,
    config: RegistrationConfig,
    folder_id: String,
}

impl UserDirectory {
    pub async fn new(config: RegistrationConfig) -> Result<Self, RegistrationError> {
        let hub = DriveAuth::create_hub(&config.credentials_path).await?;

        // Just store the folder ID; assume it's valid.
        let folder_id = config.users_folder_id.clone();

        info!("UserDirectory initialized with folder_id={}", folder_id);

        Ok(Self {
            hub,
            config,
            folder_id,
        })
    }

    fn is_shared_drive(&self) -> bool {
        self.config.shared_drive_id.is_some()
    }

    fn get_folder_id(&self) -> &str {
        &self.folder_id
    }

    pub async fn register_user(&self, user: &UserInfo) -> Result<String, RegistrationError> {
        user.validate()
            .map_err(RegistrationError::ValidationError)?;
        
        // Check if username already exists in Google Drive
        if self.find_user_by_username(&user.username).await?.is_some() {
            return Err(RegistrationError::UserAlreadyExists(user.username.clone()));
        }
    
        let folder_id = self.get_folder_id();
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
        let folder_id = self.get_folder_id();

        let query = format!(
            "'{}' in parents and mimeType = 'application/json' and trashed = false",
            folder_id
        );

        let mut request = self
            .hub
            .files()
            .list()
            .q(&query)
            .param("fields", "files(id, name)");

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
                            warn!(
                                "Failed to read user file {}: {}",
                                file.name.as_deref().unwrap_or("unknown"),
                                e
                            );
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
            .map_err(|e| {
                RegistrationError::DriveApiError(format!("Failed to read response: {}", e))
            })?
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
