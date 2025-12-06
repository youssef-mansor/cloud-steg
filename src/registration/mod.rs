//! User registration module using Firebase Storage

pub mod auth;
pub mod config;
pub mod error;
pub mod image_storage;  // NEW
pub mod user_directory;
pub mod user_info;

pub use auth::FirebaseAuth;
pub use config::RegistrationConfig;
pub use error::RegistrationError;
pub use image_storage::ImageStorage;  // NEW
pub use user_directory::UserDirectory;
pub use user_info::{UserInfo, UserStatus};
