//! Google Drive authentication

use crate::registration::error::RegistrationError;
use google_drive3::DriveHub;
use google_drive3::hyper_util::client::legacy::connect::HttpConnector;
use hyper_rustls::HttpsConnector;
use std::path::Path;
use yup_oauth2::{ServiceAccountAuthenticator, ServiceAccountKey};

pub type AuthenticatedHub = DriveHub<HttpsConnector<HttpConnector>>;

pub struct DriveAuth;

impl DriveAuth {
    pub async fn create_hub(
        credentials_path: impl AsRef<Path>,
    ) -> Result<AuthenticatedHub, RegistrationError> {
        let credentials_path = credentials_path.as_ref();

        let _ = rustls::crypto::ring::default_provider().install_default();

        let key_data = std::fs::read_to_string(credentials_path).map_err(|e| {
            RegistrationError::CredentialsFileError(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read credentials file '{}': {}",
                    credentials_path.display(),
                    e
                ),
            ))
        })?;

        let service_account_key: ServiceAccountKey =
            serde_json::from_str(&key_data).map_err(|e| {
                RegistrationError::AuthError(format!(
                    "Failed to parse service account key: {}",
                    e
                ))
            })?;

        let auth = ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await
            .map_err(|e| {
                RegistrationError::AuthError(format!("Failed to build authenticator: {}", e))
            })?;

        let hub = DriveHub::new(
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build(
                    hyper_rustls::HttpsConnectorBuilder::new()
                        .with_native_roots()
                        .unwrap()
                        .https_or_http()
                        .enable_http1()
                        .enable_http2()
                        .build(),
                ),
            auth,
        );

        Ok(hub)
    }
}
