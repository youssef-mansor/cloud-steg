//! Standalone Google Drive upload/download test

use anyhow::Result;
use google_drive3::api::File;
use google_drive3::DriveHub;
use google_drive3::hyper_util::client::legacy::connect::HttpConnector;
use hyper_rustls::HttpsConnector;
use http_body_util::BodyExt;
use std::io::Cursor;
use yup_oauth2::{ServiceAccountAuthenticator, ServiceAccountKey};

type AuthenticatedHub = DriveHub<HttpsConnector<HttpConnector>>;

async fn create_hub(credentials_path: &str) -> Result<AuthenticatedHub> {
    println!("🔐 Step 1: Loading credentials from {}", credentials_path);
    
    let _ = rustls::crypto::ring::default_provider().install_default();
    
    let key_data = std::fs::read_to_string(credentials_path)?;
    println!("✅ Credentials file read successfully");
    
    let service_account_key: ServiceAccountKey = serde_json::from_str(&key_data)?;
    println!("✅ Credentials parsed successfully");
    println!("   Service account email: {}", service_account_key.client_email);
    
    let auth = ServiceAccountAuthenticator::builder(service_account_key)
        .build()
        .await?;
    println!("✅ Authenticator built");
    
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
    
    println!("✅ DriveHub created successfully\n");
    Ok(hub)
}

async fn upload_test_file(
    hub: &AuthenticatedHub,
    folder_id: &str,
    shared_drive_id: Option<&str>,
) -> Result<String> {
    println!("📤 Step 2: Uploading test file");
    println!("   Target folder ID: {}", folder_id);
    if let Some(drive_id) = shared_drive_id {
        println!("   Shared drive ID: {}", drive_id);
    }
    
    let content = "Hello from Drive API test! Timestamp: {}";
    let content = content.replace("{}", &chrono::Utc::now().to_rfc3339());
    
    let mut file_metadata = File::default();
    file_metadata.name = Some("test-file.txt".to_string());
    file_metadata.mime_type = Some("text/plain".to_string());
    file_metadata.parents = Some(vec![folder_id.to_string()]);
    
    println!("   File name: test-file.txt");
    println!("   Content length: {} bytes", content.len());
    
    let cursor = Cursor::new(content.into_bytes());
    
    let mut request = hub
        .files()
        .create(file_metadata)
        .param("fields", "id, name, parents, webViewLink");
    
    if shared_drive_id.is_some() {
        println!("   Adding shared drive parameters...");
        request = request.supports_all_drives(true);
    }
    
    println!("   Executing upload...");
    let result = request
        .upload(cursor, "text/plain".parse().unwrap())
        .await?;
    
    let file_id = result.1.id.clone().unwrap();
    println!("✅ File uploaded successfully!");
    println!("   File ID: {}", file_id);
    println!("   File name: {}", result.1.name.as_ref().unwrap());
    if let Some(link) = result.1.web_view_link {
        println!("   View link: {}", link);
    }
    println!();
    
    Ok(file_id)
}

async fn list_files_in_folder(
    hub: &AuthenticatedHub,
    folder_id: &str,
    shared_drive_id: Option<&str>,
) -> Result<()> {
    println!("📋 Step 3: Listing files in folder");
    println!("   Folder ID: {}", folder_id);
    
    let query = format!("'{}' in parents and trashed = false", folder_id);
    println!("   Query: {}", query);
    
    let mut request = hub
        .files()
        .list()
        .q(&query)
        .param("fields", "files(id, name, mimeType, createdTime)")
        .page_size(100);
    
    if let Some(drive_id) = shared_drive_id {
        println!("   Using shared drive: {}", drive_id);
        request = request
            .corpora("drive")
            .drive_id(drive_id)
            .include_items_from_all_drives(true)
            .supports_all_drives(true);
    }
    
    println!("   Executing query...");
    let result = request.doit().await?;
    
    if let Some(files) = result.1.files {
        println!("✅ Found {} file(s):", files.len());
        for (i, file) in files.iter().enumerate() {
            println!("   {}. Name: {}", i + 1, file.name.as_ref().unwrap_or(&"<no name>".to_string()));
            println!("      ID: {}", file.id.as_ref().unwrap_or(&"<no id>".to_string()));
            println!("      Type: {}", file.mime_type.as_ref().unwrap_or(&"<unknown>".to_string()));
            if let Some(created) = &file.created_time {
                println!("      Created: {}", created);
            }
        }
    } else {
        println!("⚠️  No files found in folder");
    }
    println!();
    
    Ok(())
}

async fn download_file(
    hub: &AuthenticatedHub,
    file_id: &str,
    shared_drive_id: Option<&str>,
) -> Result<String> {
    println!("📥 Step 4: Downloading file");
    println!("   File ID: {}", file_id);
    
    let mut request = hub.files().get(file_id).param("alt", "media");
    
    if shared_drive_id.is_some() {
        println!("   Adding shared drive parameters...");
        request = request.supports_all_drives(true);
    }
    
    println!("   Executing download...");
    let response = request.doit().await?;
    
    let body_bytes = response.0.into_body().collect().await?.to_bytes();
    let content = String::from_utf8(body_bytes.to_vec())?;
    
    println!("✅ File downloaded successfully!");
    println!("   Content length: {} bytes", content.len());
    println!("   Content: {}", content);
    println!();
    
    Ok(content)
}

async fn delete_test_file(
    hub: &AuthenticatedHub,
    file_id: &str,
    shared_drive_id: Option<&str>,
) -> Result<()> {
    println!("🗑️  Step 5: Cleaning up - deleting test file");
    println!("   File ID: {}", file_id);
    
    let mut request = hub.files().delete(file_id);
    
    if shared_drive_id.is_some() {
        request = request.supports_all_drives(true);
    }
    
    request.doit().await?;
    println!("✅ Test file deleted successfully\n");
    
    Ok(())
}

async fn get_file_metadata(
    hub: &AuthenticatedHub,
    file_id: &str,
    shared_drive_id: Option<&str>,
) -> Result<()> {
    println!("📋 Step 2.5: Getting file metadata directly");
    println!("   File ID: {}", file_id);
    
    let mut request = hub
        .files()
        .get(file_id)
        .param("fields", "id, name, parents, mimeType, createdTime");
    
    if shared_drive_id.is_some() {
        request = request.supports_all_drives(true);
    }
    
    let result = request.doit().await?;
    let file = result.1;
    
    println!("✅ File metadata retrieved:");
    println!("   ID: {}", file.id.as_ref().unwrap());
    println!("   Name: {}", file.name.as_ref().unwrap());
    println!("   Parents: {:?}", file.parents);
    println!("   Type: {}", file.mime_type.as_ref().unwrap());
    println!();
    
    Ok(())
}


#[tokio::main]
async fn main() -> Result<()> {
    println!("\n==========================================");
    println!("Google Drive API Upload/Download Test");
    println!("==========================================\n");
    
    // Get configuration from environment
    let credentials_path = std::env::var("GOOGLE_CREDENTIALS")
        .unwrap_or_else(|_| "credentials/service-account.json".to_string());
    
    let folder_id = std::env::var("REGISTERED_USERS_FOLDER_ID")
        .expect("REGISTERED_USERS_FOLDER_ID must be set");
    
    let shared_drive_id = std::env::var("SHARED_DRIVE_ID").ok();
    
    println!("Configuration:");
    println!("  Credentials: {}", credentials_path);
    println!("  Target folder ID: {}", folder_id);
    println!("  Shared drive ID: {}", shared_drive_id.as_ref().unwrap_or(&"<not set>".to_string()));
    println!();
    
    // Create authenticated hub
    let hub = create_hub(&credentials_path).await?;


    // Upload test file
    let file_id = upload_test_file(&hub, &folder_id, shared_drive_id.as_deref()).await?;
    
    // Wait for propagation
    println!("⏳ Waiting 2 seconds for Drive API propagation...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Try to get file metadata directly by ID (not via search)
    get_file_metadata(&hub, &file_id, shared_drive_id.as_deref()).await?;

    // Try downloading immediately
    let content = download_file(&hub, &file_id, shared_drive_id.as_deref()).await?;

    // NOW try listing
    list_files_in_folder(&hub, &folder_id, shared_drive_id.as_deref()).await?;

    
    // Upload test file
    //let file_id = upload_test_file(&hub, &folder_id, shared_drive_id.as_deref()).await?;
    
    // List files to verify upload
    //list_files_in_folder(&hub, &folder_id, shared_drive_id.as_deref()).await?;
    
    // Download the file we just uploaded
    //let content = download_file(&hub, &file_id, shared_drive_id.as_deref()).await?;
    
    // Verify content matches
    if content.contains("Hello from Drive API test!") {
        println!("✅ Content verification PASSED - downloaded content matches!");
    } else {
        println!("❌ Content verification FAILED");
    }
    println!();
    
    // Clean up
    delete_test_file(&hub, &file_id, shared_drive_id.as_deref()).await?;
    
    // List again to verify deletion
    println!("📋 Verifying deletion:");
    list_files_in_folder(&hub, &folder_id, shared_drive_id.as_deref()).await?;
    
    println!("==========================================");
    println!("✅ ALL TESTS PASSED!");
    println!("==========================================\n");
    
    Ok(())
}
