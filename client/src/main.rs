mod steganography;

use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use image::{ImageFormat, DynamicImage, GenericImageView};
use base64::{Engine as _, engine::general_purpose};
use std::env;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use actix_web::{web, App, HttpServer, HttpResponse, Result as ActixResult};
use steganography::{ImageMetadata, encode_image_with_metadata, decode_image_with_metadata};

#[derive(Parser)]
#[command(name = "p2p-client")]
#[command(about = "P2P Discovery Client CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new user
    Register {
        #[arg(long)]
        username: String,
        #[arg(long)]
        ip: String,
        #[arg(long)]
        port: u16,
        #[arg(long, value_delimiter = ',')]
        image_paths: Vec<String>,
        #[arg(long, default_value = "http://localhost:8000")]
        server: String,
    },
    /// Start sending periodic heartbeats
    StartHeartbeat {
        #[arg(long)]
        username: String,
        #[arg(long, default_value = "http://localhost:8000")]
        server: String,
        #[arg(long, default_value = "5")]
        interval: u64,
        #[arg(long)]
        ip: String,
        #[arg(long)]
        port: u16,
    },
    /// List all online users
    ListOnline {
        #[arg(long, default_value = "http://localhost:8000")]
        server: String,
    },
    /// Request an image from another user (just sends request, doesn't receive image)
    RequestImage {
        #[arg(long)]
        username: String, // Your username
        #[arg(long)]
        target_username: String, // User whose image you want
        #[arg(long)]
        target_ip: String,
        #[arg(long)]
        target_port: u16,
        #[arg(long)]
        image_index: usize, // Which image (0-indexed)
        #[arg(long, default_value = "http://localhost:8000")]
        server: String,
    },
    /// Send/approve an image request with chosen view count
    SendImage {
        #[arg(long)]
        username: String, // Your username (image owner)
        #[arg(long)]
        requester_username: String, // Who requested the image
        #[arg(long)]
        image_index: usize, // Which image to send
        #[arg(long)]
        views: u32, // Number of views to allow
        #[arg(long)]
        ip: String, // Your IP
        #[arg(long)]
        port: u16, // Your port
    },
    /// List pending image requests
    ListRequests {
        #[arg(long)]
        username: String,
        #[arg(long)]
        ip: String,
        #[arg(long)]
        port: u16,
    },
    /// View a requested image (decrypts and decrements view count)
    ViewImage {
        #[arg(long)]
        username: String,
        #[arg(long)]
        encrypted_image_path: String,
    },
    /// Start P2P server to receive image requests
    StartP2PServer {
        #[arg(long)]
        username: String,
        #[arg(long)]
        ip: String,
        #[arg(long)]
        port: u16,
    },
    /// List received encrypted images
    ListReceivedImages {
        #[arg(long)]
        username: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    ip: String,
    port: u16,
    sample_images: Vec<String>, // array of base64 encoded 128x128 images
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    status: String,
}

#[derive(Debug, Serialize)]
struct HeartbeatRequest {
    username: String,
    ip: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct HeartbeatResponse {
    status: String,
    last_seen: String,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    username: String,
    ip: String,
    port: u16,
    sample_images: Option<Vec<String>>, // array of base64 encoded 128x128 images
}

#[derive(Debug, Deserialize)]
struct DiscoveryResponse {
    online: Vec<UserInfo>,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    status: String,
    is_leader: Option<bool>,
    current_leader: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientConfig {
    username: String,
    server: String,
}

fn process_image(image_path: &str) -> anyhow::Result<String> {
    // Load the image
    let img = image::open(image_path)
        .map_err(|e| anyhow::anyhow!("Failed to open image: {}", e))?;
    
    // Resize to 128x128
    let resized = img.resize_exact(128, 128, image::imageops::FilterType::Lanczos3);
    
    // Convert to PNG bytes
    let mut buffer = Vec::new();
    resized.write_to(&mut std::io::Cursor::new(&mut buffer), ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("Failed to encode image: {}", e))?;
    
    // Encode to base64
    let base64_image = general_purpose::STANDARD.encode(&buffer);
    
    Ok(base64_image)
}

fn save_client_config(username: &str, server: &str) -> anyhow::Result<()> {
    let config = ClientConfig {
        username: username.to_string(),
        server: server.to_string(),
    };
    
    let config_file = format!("data/client_{}.json", username);
    if let Some(parent) = Path::new(&config_file).parent() {
        fs::create_dir_all(parent)?;
    }
    
    let json = serde_json::to_string_pretty(&config)?;
    fs::write(&config_file, json)?;
    println!("Saved client config to {}", config_file);
    Ok(())
}

// Find the leader server by trying multiple servers
async fn find_leader_server(base_servers: &[String]) -> anyhow::Result<String> {
    let client = Client::new();
    
    // Try each server to find the leader
    for server in base_servers {
        match client.get(server).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(status) = resp.json::<StatusResponse>().await {
                        if status.is_leader == Some(true) {
                            println!("Found leader at: {}", server);
                            return Ok(server.clone());
                        }
                    }
                }
            }
            Err(_) => {
                // Server not available, try next
                continue;
            }
        }
    }
    
    anyhow::bail!("Could not find leader server. Tried: {:?}", base_servers)
}

async fn register(username: String, ip: String, port: u16, image_paths: Vec<String>, server: String) -> anyhow::Result<()> {
    let client = Client::new();
    
    // Extract base URL and try to find leader
    let base_url = server.trim_end_matches('/');
    let servers_to_try = if base_url.contains("localhost") || base_url.contains("127.0.0.1") {
        // Try common ports for local testing
        vec![
            format!("http://localhost:3000"),
            format!("http://localhost:3001"),
            format!("http://localhost:3002"),
        ]
    } else {
        // Use provided server as single option
        vec![base_url.to_string()]
    };
    
    // Find leader server
    let leader_server = find_leader_server(&servers_to_try).await?;
    
    println!("Processing {} image(s)...", image_paths.len());
    let mut sample_images = Vec::new();
    
    for (i, image_path) in image_paths.iter().enumerate() {
        println!("  [{}] Processing: {}", i + 1, image_path);
        let sample_image = process_image(image_path)?;
        println!("  [{}] Success (base64 length: {})", i + 1, sample_image.len());
        sample_images.push(sample_image);
    }
    
    println!("All {} image(s) processed successfully!", sample_images.len());
    
    let req = RegisterRequest {
        username: username.clone(),
        ip,
        port,
        sample_images,
    };
    
    let url = format!("{}/register", leader_server);
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;
    
    if resp.status().is_success() {
        let body: RegisterResponse = resp.json().await?;
        println!("Registration successful: {:?}", body);
        save_client_config(&username, &leader_server)?;
        
        // Save original images for P2P sharing
        let images_dir = format!("data/original_images/{}", username);
        fs::create_dir_all(&images_dir)?;
        for (i, image_path) in image_paths.iter().enumerate() {
            let original_img = image::open(image_path)?;
            let save_path = format!("{}/image_{}.png", images_dir, i);
            original_img.save(&save_path)?;
        }
        println!("💾 Original images saved for P2P sharing");
        
        Ok(())
    } else if resp.status().as_u16() == 400 {
        let body: serde_json::Value = resp.json().await?;
        eprintln!("Registration failed: {:?}", body);
        anyhow::bail!("User already exists");
    } else {
        let status = resp.status();
        let text = resp.text().await?;
        eprintln!("Registration failed: {} - {}", status, text);
        anyhow::bail!("Registration failed with status: {}", status);
    }
}

async fn send_heartbeat(username: &str, server: &str, ip: &str, port: u16) -> anyhow::Result<()> {
    let client = Client::new();
    
    // Extract base URL and try to find leader
    let base_url = server.trim_end_matches('/');
    let servers_to_try = if base_url.contains("localhost") || base_url.contains("127.0.0.1") {
        vec![
            format!("http://localhost:3000"),
            format!("http://localhost:3001"),
            format!("http://localhost:3002"),
        ]
    } else {
        vec![base_url.to_string()]
    };
    
    // Find leader server
    let leader_server = find_leader_server(&servers_to_try).await?;
    let url = format!("{}/heartbeat", leader_server);
    
    let req = HeartbeatRequest {
        username: username.to_string(),
        ip: ip.to_string(),
        port,
    };
    
    match client
        .post(&url)
        .json(&req)
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                let body: HeartbeatResponse = resp.json().await?;
                println!("[{}] Heartbeat OK - last_seen: {}", username, body.last_seen);
                Ok(())
            } else {
                let status = resp.status();
                let text = resp.text().await?;
                eprintln!("[{}] Heartbeat failed: {} - {}", username, status, text);
                Err(anyhow::anyhow!("Heartbeat failed: {}", status))
            }
        }
        Err(e) => {
            eprintln!("[{}] Heartbeat error: {}", username, e);
            Err(anyhow::anyhow!("Network error: {}", e))
        }
    }
}

async fn start_heartbeat(username: String, server: String, interval: u64, ip: String, port: u16) -> anyhow::Result<()> {
    println!("Starting heartbeat for '{}' to {} (interval: {}s, {}:{})", username, server, interval, ip, port);
    println!("Press CTRL+C to stop");
    
    let interval_duration = Duration::from_secs(interval);
    
    loop {
        if let Err(e) = send_heartbeat(&username, &server, &ip, port).await {
            eprintln!("Warning: {}", e);
        }
        sleep(interval_duration).await;
    }
}

fn display_image_preview(base64_str: &str, username: &str, index: usize) -> String {
    // Decode base64
    if let Ok(decoded) = general_purpose::STANDARD.decode(base64_str) {
        if let Ok(img) = image::load_from_memory(&decoded) {
            // Save to temp file
            let temp_dir = env::temp_dir();
            let filename = format!("p2p_preview_{}_{}.png", username, index);
            let temp_path = temp_dir.join(&filename);
            
            // Save original size (128x128) for better quality
            if let Ok(_) = img.save(&temp_path) {
                // Try iTerm2 inline image protocol
                let term = env::var("TERM_PROGRAM").unwrap_or_default();
                if term == "iTerm.app" {
                    // Read file and encode for iTerm2
                    if let Ok(file_data) = fs::read(&temp_path) {
                        let base64_file = general_purpose::STANDARD.encode(&file_data);
                        return format!("\x1b]1337;File=inline=1:{}\x07", base64_file);
                    }
                }
                
                // Return path - user can open manually
                return format!("💾 {}", temp_path.display());
            }
        }
    }
    "Preview unavailable".to_string()
}

fn decode_image_info(base64_str: &str) -> String {
    // Decode base64 to get image info
    if let Ok(decoded) = general_purpose::STANDARD.decode(base64_str) {
        if let Ok(img) = image::load_from_memory(&decoded) {
            let (width, height) = img.dimensions();
            let size_kb = decoded.len() / 1024;
            format!("{}x{}px ({:.1}KB)", width, height, size_kb)
        } else {
            format!("{} bytes", decoded.len())
        }
    } else {
        "invalid base64".to_string()
    }
}

async fn list_online(server: String) -> anyhow::Result<()> {
    let client = Client::new();
    
    // Extract base URL and try to find leader
    let base_url = server.trim_end_matches('/');
    let servers_to_try = if base_url.contains("localhost") || base_url.contains("127.0.0.1") {
        vec![
            format!("http://localhost:3000"),
            format!("http://localhost:3001"),
            format!("http://localhost:3002"),
        ]
    } else {
        vec![base_url.to_string()]
    };
    
    // Find leader server
    let leader_server = find_leader_server(&servers_to_try).await?;
    let url = format!("{}/discover", leader_server);
    
    let resp = client
        .get(&url)
        .send()
        .await?;
    
    if resp.status().is_success() {
        let body: DiscoveryResponse = resp.json().await?;
        if body.online.is_empty() {
            println!("No users online");
        } else {
            println!("Online users ({}):", body.online.len());
            println!();
            for user in body.online {
                println!("  👤 {} @ {}:{}", user.username, user.ip, user.port);
                if let Some(imgs) = &user.sample_images {
                    if imgs.is_empty() {
                        println!("     📷 No images");
                    } else {
                        println!("     📷 {} image(s):", imgs.len());
                        let mut preview_paths = Vec::new();
                        for (i, img_base64) in imgs.iter().enumerate() {
                            let info = decode_image_info(img_base64);
                            println!("        [{}] {}", i + 1, info);
                            
                            // Try to display actual image preview
                            let preview = display_image_preview(img_base64, &user.username, i);
                            if preview.starts_with("\x1b]") {
                                // iTerm2 image protocol - print directly
                                print!("        ");
                                print!("{}", preview);
                                println!();
                            } else if preview.starts_with("💾") {
                                // Extract path and show it
                                let path = preview.strip_prefix("💾 ").unwrap_or("");
                                println!("        {}", preview);
                                preview_paths.push(path.to_string());
                            } else {
                                println!("        {}", preview);
                            }
                        }
                        
                        // On macOS, offer to open images
                        if cfg!(target_os = "macos") && !preview_paths.is_empty() {
                            println!("        💡 Run: open {}", preview_paths[0]);
                        }
                    }
                } else {
                    println!("     📷 No images");
                }
                println!();
            }
        }
        Ok(())
    } else {
        let status = resp.status();
        let text = resp.text().await?;
        eprintln!("Failed to get online users: {} - {}", status, text);
        anyhow::bail!("Request failed with status: {}", status);
    }
}

// P2P Image Request Structures
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ImageRequest {
    requester_username: String,
    image_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageResponse {
    success: bool,
    encrypted_image_base64: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendImageRequest {
    requester_username: String,
    image_index: usize,
    views: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PendingRequest {
    requester_username: String,
    image_index: usize,
    timestamp: u64,
}

// Request an image from another user (just sends request, doesn't receive image)
async fn request_image(
    username: String,
    target_username: String,
    target_ip: String,
    target_port: u16,
    image_index: usize,
    _server: String,
) -> anyhow::Result<()> {
    println!("📤 Sending image request [{}] to {} @ {}:{}", image_index, target_username, target_ip, target_port);
    
    // For local testing, use 127.0.0.1 if IP is a local network IP
    let connect_ip = if target_ip.starts_with("192.168.") || target_ip.starts_with("10.") {
        "127.0.0.1"
    } else {
        &target_ip
    };
    
    // Send request to target client
    let client = Client::new();
    let url = format!("http://{}:{}/p2p/request-image", connect_ip, target_port);
    
    let req = ImageRequest {
        requester_username: username.clone(),
        image_index,
    };
    
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;
    
    let status = resp.status();
    if status.is_success() {
        let body: ImageResponse = resp.json().await?;
        if body.success {
            println!("✅ Request sent successfully!");
            println!("💡 {} needs to approve and send the image using 'send-image' command", target_username);
        } else {
            eprintln!("❌ Request failed: {}", body.error.unwrap_or_else(|| "Unknown error".to_string()));
        }
    } else {
        let text = resp.text().await?;
        eprintln!("❌ Request failed: {} - {}", status, text);
        anyhow::bail!("Request failed");
    }
    
    Ok(())
}

// Send/approve an image request with chosen view count
async fn send_image(
    username: String,
    requester_username: String,
    image_index: usize,
    views: u32,
    ip: String,
    port: u16,
) -> anyhow::Result<()> {
    println!("📤 Sending image [{}] to {} with {} views", image_index, requester_username, views);
    
    // First, get encrypted image from our P2P server
    let connect_ip = if ip.starts_with("192.168.") || ip.starts_with("10.") {
        "127.0.0.1"
    } else {
        &ip
    };
    
    let client = Client::new();
    let url = format!("http://{}:{}/p2p/send-image", connect_ip, port);
    
    let req = SendImageRequest {
        requester_username: requester_username.clone(),
        image_index,
        views,
    };
    
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;
    
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await?;
        eprintln!("❌ Failed to encrypt image: {} - {}", status, text);
        anyhow::bail!("Failed to encrypt image");
    }
    
    let body: ImageResponse = resp.json().await?;
    let encrypted_base64 = body.encrypted_image_base64.ok_or_else(|| anyhow::anyhow!("No encrypted image returned"))?;
    
    // Now get requester's IP and port from discovery server
    let servers_to_try = vec![
        "http://localhost:3000".to_string(),
        "http://localhost:3001".to_string(),
        "http://localhost:3002".to_string(),
    ];
    let leader = find_leader_server(&servers_to_try).await?;
    let discovery_url = format!("{}/discover", leader);
    let discovery_resp = client.get(&discovery_url).send().await?;
    let discovery: DiscoveryResponse = discovery_resp.json().await?;
    
    let requester_info = discovery.online.iter()
        .find(|u| u.username == requester_username)
        .ok_or_else(|| anyhow::anyhow!("Requester '{}' not found in discovery", requester_username))?;
    
    // Send encrypted image to requester's P2P server
    let requester_connect_ip = if requester_info.ip.starts_with("192.168.") || requester_info.ip.starts_with("10.") {
        "127.0.0.1"
    } else {
        &requester_info.ip
    };
    
    let receive_url = format!("http://{}:{}/p2p/receive-image", requester_connect_ip, requester_info.port);
    let receive_resp = client
        .post(&receive_url)
        .json(&ImageResponse {
            success: true,
            encrypted_image_base64: Some(encrypted_base64.clone()),
            error: None,
        })
        .send()
        .await?;
    
    if receive_resp.status().is_success() {
        println!("✅ Encrypted image sent to {} @ {}:{}", requester_username, requester_info.ip, requester_info.port);
    } else {
        let status = receive_resp.status();
        let text = receive_resp.text().await.unwrap_or_else(|_| "No error message".to_string());
        eprintln!("⚠️  Warning: Failed to deliver to requester: {} - {}", status, text);
        
        // Save encrypted image locally as fallback so requester can retrieve it
        let fallback_dir = format!("data/encrypted_images/{}", requester_username);
        if let Err(e) = fs::create_dir_all(&fallback_dir) {
            eprintln!("❌ Failed to create fallback directory: {}", e);
        } else {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let fallback_path = format!("{}/from_{}_image_{}_{}.png", fallback_dir, username, image_index, timestamp);
            
            match general_purpose::STANDARD.decode(&encrypted_base64) {
                Ok(decoded) => {
                    if let Err(e) = fs::write(&fallback_path, decoded) {
                        eprintln!("❌ Failed to save fallback image: {}", e);
                    } else {
                        println!("💾 Encrypted image saved locally as fallback: {}", fallback_path);
                        println!("💡 {} can retrieve it from: {}", requester_username, fallback_path);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to decode base64 for fallback: {}", e);
                }
            }
        }
    }
    
    Ok(())
}

// List pending image requests
async fn list_requests(username: String, ip: String, port: u16) -> anyhow::Result<()> {
    // For local testing, use 127.0.0.1 if IP is a local network IP
    let connect_ip = if ip.starts_with("192.168.") || ip.starts_with("10.") {
        "127.0.0.1"
    } else {
        &ip
    };
    
    let client = Client::new();
    let url = format!("http://{}:{}/p2p/list-requests", connect_ip, port);
    
    let resp = client
        .get(&url)
        .send()
        .await?;
    
    let status = resp.status();
    if status.is_success() {
        let requests: Vec<PendingRequest> = resp.json().await?;
        if requests.is_empty() {
            println!("📭 No pending requests for {}", username);
        } else {
            println!("📬 Pending requests for {}:", username);
            for (i, req) in requests.iter().enumerate() {
                println!("  [{}] {} requests image [{}] (requested {}s ago)", 
                    i, req.requester_username, req.image_index,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() - req.timestamp);
            }
            println!("\n💡 Use 'send-image' command to approve and send an image");
        }
    } else {
        let text = resp.text().await?;
        eprintln!("❌ Failed to list requests: {} - {}", status, text);
        anyhow::bail!("Failed to list requests");
    }
    
    Ok(())
}

// List received encrypted images
async fn list_received_images(username: String) -> anyhow::Result<()> {
    let encrypted_dir = format!("data/encrypted_images/{}", username);
    
    if !Path::new(&encrypted_dir).exists() {
        println!("📭 No encrypted images directory found for {}", username);
        println!("💡 Images will appear here after you receive them from other users");
        return Ok(());
    }
    
    let mut images = Vec::new();
    let dir = fs::read_dir(&encrypted_dir)?;
    
    for entry in dir {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "png" {
                    if let Some(filename) = path.file_name() {
                        images.push(filename.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    
    if images.is_empty() {
        println!("📭 No encrypted images found for {}", username);
    } else {
        println!("📬 Encrypted images for {}:", username);
        println!("   Directory: {}", encrypted_dir);
        println!("");
        for (i, img) in images.iter().enumerate() {
            let full_path = format!("{}/{}", encrypted_dir, img);
            let file_size = fs::metadata(&full_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let size_kb = file_size / 1024;
            println!("  [{}] {} ({}KB)", i, img, size_kb);
            println!("      Path: {}", full_path);
        }
        println!("");
        println!("💡 Use 'view-image' command to decrypt and view an image");
    }
    
    Ok(())
}

// View an encrypted image (decrypts, verifies, decrements view count, re-encrypts)
async fn view_image(username: String, encrypted_image_path: String) -> anyhow::Result<()> {
    println!("Decrypting image: {}", encrypted_image_path);
    
    // Load encrypted image
    let stego_image = image::open(&encrypted_image_path)
        .map_err(|e| anyhow::anyhow!("Failed to open encrypted image: {}", e))?;
    
    // Decode metadata and image
    let (metadata, secret_image) = decode_image_with_metadata(stego_image)
        .map_err(|e| anyhow::anyhow!("Failed to decode: {}", e))?;
    
    // Verify username matches
    if metadata.allowed_username != username {
        anyhow::bail!("Access denied: This image is encrypted for '{}', not '{}'", 
            metadata.allowed_username, username);
    }
    
    // Check view count
    if metadata.views_remaining == 0 {
        anyhow::bail!("Access denied: No views remaining for this image");
    }
    
    println!("✅ Access granted!");
    println!("   Owner: {}", metadata.original_username);
    println!("   Views remaining: {}", metadata.views_remaining);
    
    // Decrement view count
    let mut new_metadata = metadata.clone();
    new_metadata.views_remaining -= 1;
    
    // Save decrypted image temporarily
    let temp_dir = env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let temp_path = temp_dir.join(format!("p2p_decrypted_{}_{}.png", username, timestamp));
    secret_image.save(&temp_path)?;
    
    println!("✅ Image decrypted and saved to: {}", temp_path.display());
    
    // Re-encrypt with updated view count if views remain
    if new_metadata.views_remaining > 0 {
        // Load a cover image (use the encrypted image itself as cover, or create a simple one)
        // For simplicity, we'll use the stego image as cover
        let cover_image = image::open(&encrypted_image_path)?;
        let re_encrypted = encode_image_with_metadata(cover_image, secret_image, &new_metadata);
        
        // Save re-encrypted image
        re_encrypted.save(&encrypted_image_path)?;
        println!("✅ Image re-encrypted with {} views remaining", new_metadata.views_remaining);
    } else {
        // Delete encrypted image when views reach 0
        fs::remove_file(&encrypted_image_path)?;
        println!("⚠️  Views exhausted. Encrypted image deleted.");
    }
    
    // On macOS, open the image
    if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(&temp_path)
            .spawn()
            .ok();
    }
    
    Ok(())
}

// P2P Server State
struct P2PServerState {
    username: String,
    images: Arc<Mutex<Vec<DynamicImage>>>, // Original images from registration
    pending_requests: Arc<Mutex<Vec<PendingRequest>>>, // Pending image requests
    requester_info: Arc<Mutex<HashMap<String, (String, u16)>>>, // requester_username -> (ip, port)
}

// Start P2P server to receive image requests
async fn start_p2p_server(username: String, ip: String, port: u16) -> anyhow::Result<()> {
    println!("Starting P2P server for {} on {}:{}", username, ip, port);
    println!("Press CTRL+C to stop");
    
    // Load user's original images from registration
    let images_dir = format!("data/original_images/{}", username);
    let mut images = Vec::new();
    
    if Path::new(&images_dir).exists() {
        let mut index = 0;
        loop {
            let image_path = format!("{}/image_{}.png", images_dir, index);
            if Path::new(&image_path).exists() {
                match image::open(&image_path) {
                    Ok(img) => {
                        images.push(img);
                        index += 1;
                    }
                    Err(_) => break,
                }
            } else {
                break;
            }
        }
    }
    
    if images.is_empty() {
        eprintln!("⚠️  Warning: No original images found. Register first with images.");
        eprintln!("   Images directory: {}", images_dir);
    } else {
        println!("✅ Loaded {} image(s) for sharing", images.len());
    }
    
    let state = web::Data::new(P2PServerState {
        username: username.clone(),
        images: Arc::new(Mutex::new(images)),
        pending_requests: Arc::new(Mutex::new(Vec::new())),
        requester_info: Arc::new(Mutex::new(HashMap::new())),
    });
    
    // For local testing, always bind to 0.0.0.0 (all interfaces)
    // This allows connections from localhost even if IP doesn't match
    let bind_addr = format!("0.0.0.0:{}", port);
    
    println!("Binding P2P server to: {} (accessible via {}:{})", bind_addr, ip, port);
    
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/p2p/request-image", web::post().to(handle_image_request))
            .route("/p2p/send-image", web::post().to(handle_send_image))
            .route("/p2p/list-requests", web::get().to(handle_list_requests))
            .route("/p2p/receive-image", web::post().to(handle_receive_image))
    })
    .bind(&bind_addr)?
    .run()
    .await?;
    
    Ok(())
}

// Handle incoming image request (just stores it, doesn't send image)
async fn handle_image_request(
    req: web::Json<ImageRequest>,
    state: web::Data<P2PServerState>,
) -> ActixResult<HttpResponse> {
    let requester = req.requester_username.clone();
    let image_index = req.image_index;
    
    println!("📥 Image request from '{}' for image [{}]", requester, image_index);
    
    let images = state.images.lock().await;
    
    if image_index >= images.len() {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            encrypted_image_base64: None,
            error: Some(format!("Image index {} out of range (have {} images)", image_index, images.len())),
        }));
    }
    drop(images);
    
    // Store the request
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let mut requests = state.pending_requests.lock().await;
    requests.push(PendingRequest {
        requester_username: requester.clone(),
        image_index,
        timestamp,
    });
    
    println!("✅ Request stored. Use 'list-requests' to see pending requests");
    
    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        encrypted_image_base64: None,
        error: None,
    }))
}

// Handle send image (encrypts and sends to requester)
async fn handle_send_image(
    req: web::Json<SendImageRequest>,
    state: web::Data<P2PServerState>,
) -> ActixResult<HttpResponse> {
    let requester = req.requester_username.clone();
    let image_index = req.image_index;
    let views = req.views;
    
    println!("📤 Sending image [{}] to '{}' with {} views", image_index, requester, views);
    
    let images = state.images.lock().await;
    
    if image_index >= images.len() {
        return Ok(HttpResponse::BadRequest().json(ImageResponse {
            success: false,
            encrypted_image_base64: None,
            error: Some(format!("Image index {} out of range (have {} images)", image_index, images.len())),
        }));
    }
    
    // Get the secret image
    let secret_image = images[image_index].clone();
    drop(images);
    
    // Create a cover image
    let cover_image = DynamicImage::ImageRgba8(
        image::ImageBuffer::from_fn(800, 600, |_, _| {
            image::Rgba([100, 150, 200, 255])
        })
    );
    
    // Create metadata
    let metadata = ImageMetadata {
        allowed_username: requester.clone(),
        views_remaining: views,
        original_username: state.username.clone(),
    };
    
    // Encode image with metadata
    let encrypted_image = encode_image_with_metadata(cover_image, secret_image, &metadata);
    
    // Convert to base64
    let mut buffer = Vec::new();
    encrypted_image.write_to(&mut std::io::Cursor::new(&mut buffer), ImageFormat::Png)
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Failed to encode: {}", e)))?;
    
    let base64_image = general_purpose::STANDARD.encode(&buffer);
    
    // Remove the request from pending
    let mut requests = state.pending_requests.lock().await;
    requests.retain(|r| !(r.requester_username == requester && r.image_index == image_index));
    
    println!("✅ Encrypted image ready to send to '{}'", requester);
    
    Ok(HttpResponse::Ok().json(ImageResponse {
        success: true,
        encrypted_image_base64: Some(base64_image),
        error: None,
    }))
}

// Handle list requests
async fn handle_list_requests(
    state: web::Data<P2PServerState>,
) -> ActixResult<HttpResponse> {
    let requests = state.pending_requests.lock().await;
    let requests_vec: Vec<PendingRequest> = requests.clone();
    drop(requests);
    
    Ok(HttpResponse::Ok().json(requests_vec))
}

// Handle receive image (called by sender to deliver encrypted image to requester)
async fn handle_receive_image(
    req: web::Json<ImageResponse>,
    state: web::Data<P2PServerState>,
) -> ActixResult<HttpResponse> {
    let _username = &state.username; // Keep for potential future use
    // This endpoint receives the encrypted image and saves it locally
    if let Some(encrypted_base64) = &req.encrypted_image_base64 {
        // Save encrypted image locally
        let encrypted_dir = format!("data/encrypted_images/{}", state.username);
        if let Err(e) = fs::create_dir_all(&encrypted_dir) {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to create directory: {}", e)
            })));
        }
        
        // We need to know which image this is - for now, save with timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let encrypted_path = format!("{}/received_image_{}.png", encrypted_dir, timestamp);
        
        match general_purpose::STANDARD.decode(encrypted_base64) {
            Ok(decoded) => {
                if let Err(e) = fs::write(&encrypted_path, decoded) {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": format!("Failed to save image: {}", e)
                    })));
                }
                println!("✅ Encrypted image received and saved to: {}", encrypted_path);
            }
            Err(e) => {
                return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                    "error": format!("Failed to decode base64: {}", e)
                })));
            }
        }
    }
    
    Ok(HttpResponse::Ok().json(serde_json::json!({"status": "received"})))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Register {
            username,
            ip,
            port,
            image_paths,
            server,
        } => register(username, ip, port, image_paths, server).await,
        Commands::StartHeartbeat {
            username,
            server,
            interval,
            ip,
            port,
        } => start_heartbeat(username, server, interval, ip, port).await,
        Commands::ListOnline { server } => list_online(server).await,
        Commands::RequestImage {
            username,
            target_username,
            target_ip,
            target_port,
            image_index,
            server: _server,
        } => request_image(username, target_username, target_ip, target_port, image_index, "".to_string()).await,
        Commands::ViewImage {
            username,
            encrypted_image_path,
        } => view_image(username, encrypted_image_path).await,
        Commands::StartP2PServer {
            username,
            ip,
            port,
        } => start_p2p_server(username, ip, port).await,
        Commands::SendImage {
            username,
            requester_username,
            image_index,
            views,
            ip,
            port,
        } => send_image(username, requester_username, image_index, views, ip, port).await,
        Commands::ListRequests {
            username,
            ip,
            port,
        } => list_requests(username, ip, port).await,
        Commands::ListReceivedImages {
            username,
        } => list_received_images(username).await,
    }
}

