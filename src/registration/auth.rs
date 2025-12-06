//! Firebase Storage authentication using service account

use crate::registration::error::RegistrationError;
use cloud_storage::Client;
use std::env;
use std::path::Path;

pub struct FirebaseAuth;

impl FirebaseAuth {
    /// Create a Firebase Storage client using service account credentials
    ///
    /// # Arguments
    /// * `credentials_path` - Path to the service account JSON file
    ///
    /// # Returns
    /// A configured Firebase Storage Client ready to make API calls
    pub fn create_client(credentials_path: impl AsRef<Path>) -> Result<Client, RegistrationError> {
        let credentials_path = credentials_path.as_ref();

        // Verify credentials file exists
        if !credentials_path.exists() {
            return Err(RegistrationError::CredentialsFileError(
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Credentials file not found: {}", credentials_path.display()),
                ),
            ));
        }

        // Set SERVICE_ACCOUNT env var for cloud-storage crate
        env::set_var("SERVICE_ACCOUNT", credentials_path);

        // Create client
        let client = Client::default();

        Ok(client)
    }
}
