//! Image storage for user images
//! Structure: users/{username}/images/{timestamp}-{uuid}.{ext}

use crate::registration::error::RegistrationError;
use crate::registration::user_directory::UserDirectory;
use cloud_storage::Client;
use futures::stream::StreamExt;
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;
use tracing::info;
use uuid::Uuid;

pub struct ImageStorage<'a> {
    user_directory: &'a UserDirectory,
}

impl<'a> ImageStorage<'a> {
    pub fn new(user_directory: &'a UserDirectory) -> Self {
        Self { user_directory }
    }

    /// Get the images folder path for a user
    fn get_images_folder(&self, username: &str) -> String {
        format!("users/{}/images/", username)
    }

    /// Generate a unique image filename
    fn generate_filename(&self, extension: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp();
        let uuid = Uuid::new_v4();
        format!("{}-{}.{}", timestamp, uuid, extension)
    }

    /// Upload an image for a user (must be registered and <= 128x128)
    pub async fn upload_image(
        &self,
        username: &str,
        image_data: Vec<u8>,
        format: ImageFormat,
    ) -> Result<String, RegistrationError> {
        // 1. Verify user is registered
        self.user_directory.get_user(username).await?;

        // 2. Validate image dimensions
        let img = image::load_from_memory(&image_data)
            .map_err(|e| RegistrationError::ValidationError(format!("Invalid image: {}", e)))?;

        if img.width() > 128 || img.height() > 128 {
            return Err(RegistrationError::ValidationError(format!(
                "Image too large: {}x{} (max 128x128)",
                img.width(),
                img.height()
            )));
        }

        // 3. Determine extension
        let extension = match format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::WebP => "webp",
            _ => return Err(RegistrationError::ValidationError("Unsupported format".to_string())),
        };

        // 4. Generate path and upload
        let filename = self.generate_filename(extension);
        let full_path = format!("{}{}", self.get_images_folder(username), filename);

        let mime_type = match format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::WebP => "image/webp",
            _ => "application/octet-stream",
        };

        self.user_directory
            .get_client()
            .object()
            .create(
                self.user_directory.get_bucket_name(),
                image_data,
                &full_path,
                mime_type,
            )
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to upload image: {}", e))
            })?;

        info!("Uploaded image for user '{}': {}", username, full_path);
        Ok(filename)
    }

    /// List all images for a user
    pub async fn list_images(&self, username: &str) -> Result<Vec<String>, RegistrationError> {
        // Verify user exists
        self.user_directory.get_user(username).await?;

        let images_prefix = self.get_images_folder(username);
        
        let stream = self
            .user_directory
            .get_client()
            .object()
            .list(self.user_directory.get_bucket_name(), Default::default())
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to list images: {}", e))
            })?;

        tokio::pin!(stream);

        let mut images = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(object_list) => {
                    for obj in object_list.items {
                        if obj.name.starts_with(&images_prefix) {
                            // Extract just the filename
                            if let Some(filename) = obj.name.strip_prefix(&images_prefix) {
                                images.push(filename.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(RegistrationError::FirebaseApiError(format!(
                        "Error listing images: {}",
                        e
                    )));
                }
            }
        }

        Ok(images)
    }

    /// Download a specific image
    pub async fn download_image(
        &self,
        username: &str,
        filename: &str,
    ) -> Result<Vec<u8>, RegistrationError> {
        // Verify user exists
        self.user_directory.get_user(username).await?;

        let full_path = format!("{}{}", self.get_images_folder(username), filename);

        let data = self
            .user_directory
            .get_client()
            .object()
            .download(self.user_directory.get_bucket_name(), &full_path)
            .await
            .map_err(|e| {
                if e.to_string().contains("404") {
                    RegistrationError::ValidationError(format!("Image not found: {}", filename))
                } else {
                    RegistrationError::FirebaseApiError(format!("Failed to download image: {}", e))
                }
            })?;

        Ok(data)
    }

    /// Delete a specific image
    pub async fn delete_image(
        &self,
        username: &str,
        filename: &str,
    ) -> Result<(), RegistrationError> {
        let full_path = format!("{}{}", self.get_images_folder(username), filename);

        self.user_directory
            .get_client()
            .object()
            .delete(self.user_directory.get_bucket_name(), &full_path)
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to delete image: {}", e))
            })?;

        info!("Deleted image for user '{}': {}", username, filename);
        Ok(())
    }
}
