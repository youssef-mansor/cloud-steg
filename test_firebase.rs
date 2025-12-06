//! Firebase Storage Test - Upload, Download, Edit a text file
//! 
//! Run with: cargo run --bin test_firebase

use cloud_storage::Client;
use futures::stream::StreamExt;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Firebase Storage Test ===\n");

    // Configuration
    let bucket_name = env::var("FIREBASE_BUCKET")
        .expect("FIREBASE_BUCKET environment variable must be set (e.g., your-project.appspot.com)");
    
    let service_account_path = env::var("GOOGLE_APPLICATION_CREDENTIALS")
        .unwrap_or_else(|_| "credentials/firebase-storage.json".to_string());

    println!("Bucket: {}", bucket_name);
    println!("Credentials: {}\n", service_account_path);

    // Set credentials for cloud-storage library
    env::set_var("SERVICE_ACCOUNT", &service_account_path);

    let client = Client::default();
    let file_path = "test-files/sample.txt";

    // ========================================
    // STEP 1: Upload a text file
    // ========================================
    println!("📤 Step 1: Uploading file...");
    let content = "Hello from Rust! This is a test file.";
    
    client.object().create(
        &bucket_name,
        content.as_bytes().to_vec(),
        file_path,
        "text/plain",
    ).await?;
    
    println!("✓ Uploaded to: {}", file_path);
    println!("✓ Content: '{}'\n", content);

    // ========================================
    // STEP 2: Download and read the file
    // ========================================
    println!("📥 Step 2: Downloading file...");
    
    let downloaded = client.object().download(&bucket_name, file_path).await?;
    let downloaded_text = String::from_utf8(downloaded)?;
    
    println!("✓ Downloaded from: {}", file_path);
    println!("✓ Content: '{}'\n", downloaded_text);

    // Verify content matches
    assert_eq!(content, downloaded_text, "Content mismatch!");
    println!("✓ Content verified!\n");

    // ========================================
    // STEP 3: Edit the file (delete and re-upload)
    // ========================================
    println!("✏️  Step 3: Editing file...");
    
    let edited_content = format!("{}\nEdited at: {}", downloaded_text, chrono::Utc::now());
    
    // Delete old version
    client.object().delete(&bucket_name, file_path).await?;
    
    // Upload new version
    client.object().create(
        &bucket_name,
        edited_content.as_bytes().to_vec(),
        file_path,
        "text/plain",
    ).await?;
    
    println!("✓ Updated file with new content\n");

    // ========================================
    // STEP 4: Download again to verify edit
    // ========================================
    println!("📥 Step 4: Downloading edited file...");
    
    let final_content = client.object().download(&bucket_name, file_path).await?;
    let final_text = String::from_utf8(final_content)?;
    
    println!("✓ Final content:");
    println!("{}\n", final_text);

    // ========================================
    // STEP 5: List files in bucket
    // ========================================
    println!("📋 Step 5: Listing files in bucket...");
    
    let stream = client.object().list(&bucket_name, Default::default()).await?;
    tokio::pin!(stream);  // Pin the stream to make it work with .next()
    
    println!("✓ Files in bucket:");
    while let Some(result) = stream.next().await {
        match result {
            Ok(object_list) => {
                for obj in object_list.items {
                    println!("  - {} ({} bytes)", obj.name, obj.size);
                }
            }
            Err(e) => eprintln!("Error listing: {}", e),
        }
    }
    println!();

    // ========================================
    // STEP 6: Clean up (delete test file)
    // ========================================
    println!("🗑️  Step 6: Cleaning up...");
    
    client.object().delete(&bucket_name, file_path).await?;
    
    println!("✓ Deleted: {}\n", file_path);

    println!("=== All tests passed! ===");
    
    Ok(())
}
