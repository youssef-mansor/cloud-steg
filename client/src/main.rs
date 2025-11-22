use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

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
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    status: String,
}

#[derive(Debug, Serialize)]
struct HeartbeatRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
struct HeartbeatResponse {
    status: String,
    last_seen: String,
}

#[derive(Debug, Deserialize)]
struct DiscoveryResponse {
    online: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientConfig {
    username: String,
    server: String,
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

async fn register(username: String, password: String, server: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/register", server);
    
    let req = RegisterRequest {
        username: username.clone(),
        password,
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

async fn send_heartbeat(username: &str, server: &str) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/heartbeat", server);
    
    let req = HeartbeatRequest {
        username: username.to_string(),
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

async fn start_heartbeat(username: String, server: String, interval: u64) -> anyhow::Result<()> {
    println!("Starting heartbeat for '{}' to {} (interval: {}s)", username, server, interval);
    println!("Press CTRL+C to stop");
    
    let interval_duration = Duration::from_secs(interval);
    
    loop {
        if let Err(e) = send_heartbeat(&username, &server).await {
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
            for username in body.online {
                println!("  - {}", username);
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
            server,
        } => register(username, password, server).await,
        Commands::StartHeartbeat {
            username,
            server,
            interval,
        } => start_heartbeat(username, server, interval).await,
        Commands::ListOnline { server } => list_online(server).await,
    }
}

