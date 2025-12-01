//! User registration module using Google Drive

pub mod auth;
pub mod config;
pub mod error;
pub mod user_directory;
pub mod user_info;

pub use auth::DriveAuth;
pub use config::RegistrationConfig;
pub use error::RegistrationError;
pub use user_directory::UserDirectory;
pub use user_info::{UserInfo, UserStatus};
