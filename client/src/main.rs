//! Simple Client for Cloud Steganography Server
//! 
//! Compatible with the `simple` branch server API.
//! Takes server IPs as command-line arguments.

use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Parser)]
#[command(name = "simple-client")]
#[command(about = "Simple client for distributed server cluster")]
struct Cli {
    /// Comma-separated list of server addresses (e.g., "http://10.40.6.26:3000,http://10.40.7.1:3001")
    #[arg(long, value_delimiter = ',')]
    servers: Vec<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new user
    Register {
        #[arg(long)]
        username: String,
        /// Address in format "ip:port" (e.g., "192.168.1.50:9001")
        #[arg(long)]
        addr: String,
    },
    /// Start sending periodic heartbeats
    Heartbeat {
        #[arg(long)]
        username: String,
        /// Address in format "ip:port"
        #[arg(long)]
        addr: String,
        /// Heartbeat interval in seconds
        #[arg(long, default_value = "5")]
        interval: u64,
    },
    /// List online users
    Discover,
    /// Check server status
    Status,
    /// List all registered users
    Users,
}

// ============== Request/Response Structures ==============

#[derive(Debug, Serialize)]
struct RegisterRequest {
    username: String,
    addr: String,
}

#[derive(Debug, Deserialize)]
struct RegisterResponse {
    success: bool,
    message: String,
    user_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct HeartbeatRequest {
    username: String,
    addr: String,
}

#[derive(Debug, Deserialize)]
struct HeartbeatResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    status: String,
    service: String,
    is_leader: bool,
    current_leader: Option<String>,
    online_clients_count: usize,
}

#[derive(Debug, Deserialize)]
struct DiscoveryClient {
    username: String,
    addr: String,
}

#[derive(Debug, Deserialize)]
struct DiscoveryResponse {
    online_clients: Vec<DiscoveryClient>,
    count: usize,
    is_leader: bool,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    id: String,
    username: String,
    addr: String,
}

#[derive(Debug, Deserialize)]
struct UserListResponse {
    users: Vec<UserInfo>,
    count: usize,
}

// ============== Core Functions ==============

/// Find the leader server by trying each server in the list
async fn find_leader_server(servers: &[String]) -> anyhow::Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    for server in servers {
        let url = format!("{}/", server.trim_end_matches('/'));
        
        match client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(status) = resp.json::<StatusResponse>().await {
                        if status.is_leader {
                            println!("✓ Found leader at: {}", server);
                            return Ok(server.clone());
                        } else {
                            println!("  {} is follower (leader: {:?})", server, status.current_leader);
                        }
                    }
                }
            }
            Err(e) => {
                println!("  {} unreachable: {}", server, e);
            }
        }
    }

    anyhow::bail!("Could not find leader server. Tried: {:?}", servers)
}

/// Register a new user
async fn register(servers: &[String], username: String, addr: String) -> anyhow::Result<()> {
    let leader = find_leader_server(servers).await?;
    let client = Client::new();
    
    let req = RegisterRequest {
        username: username.clone(),
        addr: addr.clone(),
    };

    let url = format!("{}/register", leader.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;

    let status = resp.status();
    let body: RegisterResponse = resp.json().await?;

    if body.success {
        println!("✅ Registration successful!");
        println!("   Username: {}", username);
        println!("   Address: {}", addr);
        if let Some(id) = body.user_id {
            println!("   User ID: {}", id);
        }
    } else {
        eprintln!("❌ Registration failed: {}", body.message);
        if status.as_u16() == 409 {
            eprintln!("   (Username already exists)");
        }
    }

    Ok(())
}

/// Send a single heartbeat
async fn send_heartbeat(leader: &str, username: &str, addr: &str) -> anyhow::Result<bool> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let req = HeartbeatRequest {
        username: username.to_string(),
        addr: addr.to_string(),
    };

    let url = format!("{}/heartbeat", leader.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await?;

    let body: HeartbeatResponse = resp.json().await?;

    if body.success {
        println!("[{}] ❤️  Heartbeat OK - {}", username, body.message);
        Ok(true)
    } else {
        eprintln!("[{}] ⚠️  Heartbeat failed: {}", username, body.message);
        Ok(false)
    }
}

/// Start heartbeat loop
async fn start_heartbeat(servers: &[String], username: String, addr: String, interval: u64) -> anyhow::Result<()> {
    println!("Starting heartbeat for '{}' at {} (interval: {}s)", username, addr, interval);
    println!("Press Ctrl+C to stop\n");

    let interval_duration = Duration::from_secs(interval);
    let mut current_leader = find_leader_server(servers).await?;

    loop {
        match send_heartbeat(&current_leader, &username, &addr).await {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                // Try to find leader again
                println!("Attempting to reconnect to leader...");
                match find_leader_server(servers).await {
                    Ok(new_leader) => {
                        current_leader = new_leader;
                    }
                    Err(e) => {
                        eprintln!("Failed to find leader: {}", e);
                    }
                }
            }
        }
        sleep(interval_duration).await;
    }
}

/// Discover online users
async fn discover(servers: &[String]) -> anyhow::Result<()> {
    let leader = find_leader_server(servers).await?;
    let client = Client::new();

    let url = format!("{}/discover", leader.trim_end_matches('/'));
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        let body: DiscoveryResponse = resp.json().await?;
        
        println!("\n📡 Online Users ({}):", body.count);
        println!("─────────────────────────────────────");
        
        if body.online_clients.is_empty() {
            println!("  (no users online)");
        } else {
            for client in body.online_clients {
                println!("  👤 {} @ {}", client.username, client.addr);
            }
        }
        println!();
    } else {
        eprintln!("❌ Discovery failed: {}", resp.status());
    }

    Ok(())
}

/// Check server status
async fn status(servers: &[String]) -> anyhow::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    println!("\n🖥️  Server Cluster Status:");
    println!("─────────────────────────────────────");

    let mut leader_found = false;

    for server in servers {
        let url = format!("{}/", server.trim_end_matches('/'));
        
        match client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(status) = resp.json::<StatusResponse>().await {
                    let role = if status.is_leader { "LEADER ⭐" } else { "follower" };
                    println!("  {} - {} | online: {}", server, role, status.online_clients_count);
                    if status.is_leader {
                        leader_found = true;
                    }
                }
            }
            Err(_) => {
                println!("  {} - OFFLINE ❌", server);
            }
        }
    }

    if !leader_found {
        println!("\n⚠️  Warning: No leader found in cluster!");
    }

    println!();
    Ok(())
}

/// List all registered users
async fn list_users(servers: &[String]) -> anyhow::Result<()> {
    let leader = find_leader_server(servers).await?;
    let client = Client::new();

    let url = format!("{}/users", leader.trim_end_matches('/'));
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        let body: UserListResponse = resp.json().await?;
        
        println!("\n📋 Registered Users ({}):", body.count);
        println!("─────────────────────────────────────");
        
        if body.users.is_empty() {
            println!("  (no users registered)");
        } else {
            for user in body.users {
                println!("  👤 {} @ {} (id: {})", user.username, user.addr, user.id);
            }
        }
        println!();
    } else {
        eprintln!("❌ Failed to list users: {}", resp.status());
    }

    Ok(())
}

// ============== Main ==============

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.servers.is_empty() {
        eprintln!("Error: At least one server address required");
        eprintln!("Usage: --servers \"http://ip1:port1,http://ip2:port2,http://ip3:port3\"");
        std::process::exit(1);
    }

    match cli.command {
        Commands::Register { username, addr } => {
            register(&cli.servers, username, addr).await?;
        }
        Commands::Heartbeat { username, addr, interval } => {
            start_heartbeat(&cli.servers, username, addr, interval).await?;
        }
        Commands::Discover => {
            discover(&cli.servers).await?;
        }
        Commands::Status => {
            status(&cli.servers).await?;
        }
        Commands::Users => {
            list_users(&cli.servers).await?;
        }
    }

    Ok(())
}
