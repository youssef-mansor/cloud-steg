use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use image::{ImageFormat, DynamicImage};
use base64::{Engine as _, engine::general_purpose};

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
        password: String,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
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

async fn register(username: String, password: String, image_paths: Vec<String>, server: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/register", server);
    
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
        password,
        sample_images,
    };
    
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;
    
    if resp.status().is_success() {
        let body: RegisterResponse = resp.json().await?;
        println!("Registration successful: {:?}", body);
        save_client_config(&username, &server)?;
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
    let url = format!("{}/heartbeat", server);
    
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

async fn list_online(server: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/discovery/online", server);
    
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
            for user in body.online {
                if let Some(imgs) = &user.sample_images {
                    if imgs.is_empty() {
                        println!("  - {} @ {}:{} [no images]", user.username, user.ip, user.port);
                    } else {
                        let preview = &imgs[0].chars().take(20).collect::<String>();
                        println!("  - {} @ {}:{} [{} image(s), first: {}...]", 
                            user.username, user.ip, user.port, imgs.len(), preview);
                    }
                } else {
                    println!("  - {} @ {}:{} [no images]", user.username, user.ip, user.port);
                }
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Register {
            username,
            password,
            image_paths,
            server,
        } => register(username, password, image_paths, server).await,
        Commands::StartHeartbeat {
            username,
            server,
            interval,
            ip,
            port,
        } => start_heartbeat(username, server, interval, ip, port).await,
        Commands::ListOnline { server } => list_online(server).await,
    }
}

