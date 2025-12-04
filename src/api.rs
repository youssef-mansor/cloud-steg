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

// Online client tracking
#[derive(Debug, Clone)]
pub struct OnlineClient {
    pub username: String,
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
    pub addr: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub message: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub service: String,
    pub is_leader: bool,
    pub current_leader: Option<String>,
    pub online_clients_count: usize,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResponse {
    pub online_clients: Vec<String>,
    pub count: usize,
    pub is_leader: bool,
}


// Configure routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route("/register", post(register_user))
        .route("/heartbeat", post(heartbeat))
        .route("/users", get(list_users))
        .route("/discover", get(discover_online))
        .with_state(state)
}

// Health check endpoint
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let ns = state.node_state.read().await;
    let is_leader = ns.state == crate::State::Leader;
    let current_leader = ns.leader.clone();
    
    let online_count = state.online_clients.read().await.len();
    
    Json(StatusResponse {
        status: "ok".to_string(),
        service: "distributed-system-registration".to_string(),
        is_leader,
        current_leader,
        online_clients_count: online_count,
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
                success: false,
                message: format!(
                    "This node is not the leader. Current leader: {}",
                    leader_addr.unwrap_or_else(|| "unknown".to_string())
                ),
                user_id: None,
            }),
        );
    }

    // Process registration (only if leader)
    info!(
        "Registration request for username: {} at addr {} (I am leader)",
        payload.username, payload.addr
    );

    let user = UserInfo::new(payload.username, payload.addr);

    match state.user_directory.register_user(&user).await {
        Ok(_) => {
            info!("Successfully registered user: {} at {}", user.username, user.addr);
            (
                StatusCode::CREATED,
                Json(RegisterResponse {
                    success: true,
                    message: format!(
                        "User '{}' registered successfully at {}",
                        user.username, user.addr
                    ),
                    user_id: Some(user.id.clone()),
                }),
            )
        }
        Err(e) => {
            tracing::error!("Registration failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(RegisterResponse {
                    success: false,
                    message: format!("Registration failed: {}", e),
                    user_id: None,
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
                success: false,
                message: format!(
                    "This node is not the leader. Current leader: {}",
                    leader_addr.unwrap_or_else(|| "unknown".to_string())
                ),
            }),
        );
    }

    // Update heartbeat timestamp
    let username = payload.username.clone();
    let mut online = state.online_clients.write().await;
    
    online.insert(
        username.clone(),
        OnlineClient {
            username: username.clone(),
            last_heartbeat: Instant::now(),
        },
    );

    info!("Heartbeat received from: {} (total online: {})", username, online.len());

    (
        StatusCode::OK,
        Json(HeartbeatResponse {
            success: true,
            message: format!("Heartbeat accepted for '{}'", username),
        }),
    )
}

// List users endpoint - ONLY LEADER CAN PROCESS
async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        info!("List users request rejected - not leader");
        return (
            StatusCode::FORBIDDEN,
            Json(UserListResponse {
                users: vec![],
                count: 0,
            }),
        );
    }

    match state.user_directory.list_users().await {
        Ok(users) => {
            let count = users.len();
            (
                StatusCode::OK,
                Json(UserListResponse { users, count }),
            )
        }
        Err(e) => {
            tracing::error!("Failed to list users: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(UserListResponse {
                    users: vec![],
                    count: 0,
                }),
            )
        }
    }
}

// Discovery endpoint - ONLY LEADER CAN PROCESS
async fn discover_online(State(state): State<AppState>) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        info!("Discovery request rejected - not leader");
        return (
            StatusCode::FORBIDDEN,
            Json(DiscoveryResponse {
                online_clients: vec![],
                count: 0,
                is_leader: false,
            }),
        );
    }

    // Return currently online clients
    let online = state.online_clients.read().await;
    let online_list: Vec<String> = online
        .values()
        .map(|client| client.username.clone())
        .collect();

    info!("Discovery request served: {} clients online", online_list.len());

    (
        StatusCode::OK,
        Json(DiscoveryResponse {
            online_clients: online_list,
            count: online.len(),
            is_leader: true,
        }),
    )
}

