//! Note storage for user images
//! Structure: users/{username}/notes/{image_filename}.json

use crate::registration::error::RegistrationError;
use crate::registration::user_directory::UserDirectory;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageNote {
    pub image_filename: String,
    pub view_count_edit: i32,
}

pub struct NoteStorage<'a> {
    user_directory: &'a UserDirectory,
}

impl<'a> NoteStorage<'a> {
    pub fn new(user_directory: &'a UserDirectory) -> Self {
        Self { user_directory }
    }

    /// Get the notes folder path for a user
    fn get_notes_folder(&self, username: &str) -> String {
        format!("users/{}/notes/", username)
    }

    /// Get the full path for a note file
    fn get_note_path(&self, username: &str, image_filename: &str) -> String {
        format!("{}{}.json", self.get_notes_folder(username), image_filename)
    }

    /// Add or update a note for a specific image
    pub async fn add_note(
        &self,
        target_username: &str,
        target_image: &str,
        view_count_edit: i32,
    ) -> Result<(), RegistrationError> {
        // 1. Verify target user exists
        self.user_directory.get_user(target_username).await?;

        // 2. Verify target image exists
        let image_path = format!("users/{}/images/{}", target_username, target_image);
        
        match self
            .user_directory
            .get_client()
            .object()
            .download(self.user_directory.get_bucket_name(), &image_path)
            .await
        {
            Ok(_) => {} // Image exists
            Err(e) => {
                if e.to_string().contains("404") || e.to_string().contains("No such object") {
                    return Err(RegistrationError::ValidationError(format!(
                        "Image not found: {}",
                        target_image
                    )));
                } else {
                    return Err(RegistrationError::FirebaseApiError(format!(
                        "Error checking image: {}",
                        e
                    )));
                }
            }
        }

        // 3. Create note object
        let note = ImageNote {
            image_filename: target_image.to_string(),
            view_count_edit,
        };

        let note_json = serde_json::to_string_pretty(&note)?;

        // 4. Upload note to Firebase
        let note_path = self.get_note_path(target_username, target_image);

        self.user_directory
            .get_client()
            .object()
            .create(
                self.user_directory.get_bucket_name(),
                note_json.as_bytes().to_vec(),
                &note_path,
                "application/json",
            )
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to add note: {}", e))
            })?;

        info!(
            "Added note for {}/{}: view_count_edit={}",
            target_username, target_image, view_count_edit
        );

        Ok(())
    }

    /// Get all notes for a user
    pub async fn get_notes(&self, username: &str) -> Result<Vec<ImageNote>, RegistrationError> {
        // Verify user exists
        self.user_directory.get_user(username).await?;

        let notes_prefix = self.get_notes_folder(username);

        let stream = self
            .user_directory
            .get_client()
            .object()
            .list(self.user_directory.get_bucket_name(), Default::default())
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to list notes: {}", e))
            })?;

        tokio::pin!(stream);

        let mut notes = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(object_list) => {
                    for obj in object_list.items {
                        if obj.name.starts_with(&notes_prefix) && obj.name.ends_with(".json") {
                            match self.download_note(&obj.name).await {
                                Ok(note) => notes.push(note),
                                Err(e) => {
                                    tracing::warn!("Failed to read note {}: {}", obj.name, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(RegistrationError::FirebaseApiError(format!(
                        "Error listing notes: {}",
                        e
                    )));
                }
            }
        }

        Ok(notes)
    }

    /// Download a specific note by path
    async fn download_note(&self, note_path: &str) -> Result<ImageNote, RegistrationError> {
        let data = self
            .user_directory
            .get_client()
            .object()
            .download(self.user_directory.get_bucket_name(), note_path)
            .await
            .map_err(|e| {
                RegistrationError::FirebaseApiError(format!("Failed to download note: {}", e))
            })?;

        let note: ImageNote = serde_json::from_slice(&data)?;
        Ok(note)
    }
}
