//! Error types for user registration

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistrationError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Failed to read credentials file: {0}")]
    CredentialsFileError(#[from] std::io::Error),

    #[error("Failed to parse credentials: {0}")]
    CredentialsParseError(#[from] serde_json::Error),

    #[error("Firebase Storage API error: {0}")]
    FirebaseApiError(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("User already exists: {0}")]
    UserAlreadyExists(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
}
