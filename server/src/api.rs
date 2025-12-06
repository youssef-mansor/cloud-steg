//! HTTP API for user registration and heartbeat tracking

use crate::registration::{UserDirectory, UserInfo};
use crate::NodeState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;
use chrono::Utc;


// Online client tracking
#[derive(Debug, Clone)]
pub struct OnlineClient {
    pub username: String,
    pub ip: String,
    pub port: u16,
    pub sample_images: Option<Vec<String>>,
    pub last_heartbeat: Instant,
}

// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub user_directory: Arc<UserDirectory>,
    pub node_state: Arc<RwLock<NodeState>>,
    pub online_clients: Arc<RwLock<HashMap<String, OnlineClient>>>,
}

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub ip: String,
    pub port: u16,
    #[serde(default)]
    pub sample_images: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub username: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub status: String,
    pub last_seen: String,
}

// Response format for discovery - matches client expectations
#[derive(Debug, Serialize)]
pub struct OnlineUserInfo {
    pub username: String,
    pub ip: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_images: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResponse {
    pub online: Vec<OnlineUserInfo>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub is_leader: Option<bool>,
    pub current_leader: Option<String>,
}

// Configure routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route("/register", post(register_user))
        .route("/heartbeat", post(heartbeat))
        .route("/discover", get(discover_online))
        .with_state(state)
}

// Health check endpoint
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let ns = state.node_state.read().await;
    let is_leader = ns.state == crate::State::Leader;
    let current_leader = ns.leader.clone();
    
    Json(StatusResponse {
        status: "ok".to_string(),
        is_leader: Some(is_leader),
        current_leader,
    })
}

// Register endpoint - ONLY LEADER CAN PROCESS
async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        info!("Registration request rejected - not leader (current leader: {:?})", leader_addr);
        return (
            StatusCode::FORBIDDEN,
            Json(RegisterResponse {
                status: format!(
                    "error: not leader. Current leader: {}",
                    leader_addr.unwrap_or_else(|| "unknown".to_string())
                ),
            }),
        );
    }

    // Process registration (only if leader)
    let addr = format!("{}:{}", payload.ip, payload.port);
    info!(
        "Registration request for username: {} at {} (I am leader)",
        payload.username, addr
    );

    let sample_images = if payload.sample_images.is_empty() {
        None
    } else {
        Some(payload.sample_images.clone())
    };
    
    let user = UserInfo::new(
        payload.username.clone(),
        payload.ip.clone(),
        payload.port,
        sample_images.clone(),
    );

    // Also add to online clients
    {
        let mut online = state.online_clients.write().await;
        online.insert(
            payload.username.clone(),
            OnlineClient {
                username: payload.username.clone(),
                ip: payload.ip.clone(),
                port: payload.port,
                sample_images,
                last_heartbeat: Instant::now(),
            },
        );
    }

    match state.user_directory.register_user(&user).await {
        Ok(_) => {
            info!("Successfully registered user: {} at {}", user.username, user.addr());
            (
                StatusCode::CREATED,
                Json(RegisterResponse {
                    status: "registered".to_string(),
                }),
            )
        }
        Err(e) => {
            tracing::error!("Registration failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(RegisterResponse {
                    status: format!("error: {}", e),
                }),
            )
        }
    }
}

// Heartbeat endpoint - ONLY LEADER CAN PROCESS
async fn heartbeat(
    State(state): State<AppState>,
    Json(payload): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        return (
            StatusCode::FORBIDDEN,
            Json(HeartbeatResponse {
                status: format!(
                    "error: not leader. Current leader: {}",
                    leader_addr.unwrap_or_else(|| "unknown".to_string())
                ),
                last_seen: Utc::now().to_rfc3339(),
            }),
        );
    }

    // Update heartbeat timestamp
    let username = payload.username.clone();
    let now = Utc::now();

    let mut online = state.online_clients.write().await;
    
    // Get existing sample_images if user exists
    let sample_images = online.get(&username).and_then(|c| c.sample_images.clone());
    
    online.insert(
        username.clone(),
        OnlineClient {
            username: username.clone(),
            ip: payload.ip.clone(),
            port: payload.port,
            sample_images,
            last_heartbeat: Instant::now(),
        },
    );

    info!(
        "Heartbeat received from: {} at {}:{} (total online: {})",
        username,
        payload.ip,
        payload.port,
        online.len()
    );

    (
        StatusCode::OK,
        Json(HeartbeatResponse {
            status: "ok".to_string(),
            last_seen: now.to_rfc3339(),
        }),
    )
}

// Discovery endpoint - ONLY LEADER CAN PROCESS
async fn discover_online(State(state): State<AppState>) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, _leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        info!("Discovery request rejected - not leader");
        return (
            StatusCode::FORBIDDEN,
            Json(DiscoveryResponse {
                online: vec![],
            }),
        );
    }

    // Return currently online clients
    let online = state.online_clients.read().await;
    let online_list: Vec<OnlineUserInfo> = online
        .values()
        .map(|client| OnlineUserInfo {
            username: client.username.clone(),
            ip: client.ip.clone(),
            port: client.port,
            sample_images: client.sample_images.clone(),
        })
        .collect();

    info!(
        "Discovery request served: {} clients online",
        online_list.len()
    );

    (
        StatusCode::OK,
        Json(DiscoveryResponse {
            online: online_list,
        }),
    )
}
