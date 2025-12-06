//! HTTP API for user registration and heartbeat tracking



use crate::registration::ImageStorage;
use axum::extract::Multipart;
use image::ImageFormat;



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
use tracing::{info, warn};  // ADD warn here
use base64::Engine;          // ADD this line



// Online client tracking
#[derive(Debug, Clone)]
pub struct OnlineClient {
    pub username: String,
    pub addr: String,
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
    pub addr: String,
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
pub struct DiscoveryClient {
    pub username: String,
    pub addr: String,      // IP:port
}

#[derive(Debug, Serialize)]
pub struct DiscoveryResponse {
    pub online_clients: Vec<DiscoveryClient>,
    pub count: usize,
    pub is_leader: bool,
}


#[derive(Debug, Serialize)]
pub struct ImageUploadResponse {
    pub success: bool,
    pub message: String,
    pub filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImageListResponse {
    pub images: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ImageWithData {
    pub filename: String,
    pub data: String,  // base64 encoded
}

#[derive(Debug, Serialize)]
pub struct OnlineClientWithImages {
    pub username: String,
    pub addr: String,
    pub images: Vec<ImageWithData>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverWithImagesResponse {
    pub online_clients: Vec<OnlineClientWithImages>,
    pub count: usize,
}




// Configure routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route("/register", post(register_user))
        .route("/heartbeat", post(heartbeat))
        .route("/users", get(list_users))
        .route("/discover", get(discover_online))
        .route("/discover_with_images", get(discover_with_images))  // NEW
        .route("/upload_image/:username", post(upload_image))
        .route("/images/:username", get(list_user_images))
        .route("/image/:username/:filename", get(download_image))
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

    // CHECK: Username must be unique in Google Drive
    info!("Checking if username '{}' already exists...", payload.username);
    
    let all_users = match state.user_directory.list_users().await {
        Ok(users) => users,
        Err(e) => {
            tracing::error!("Failed to list users for uniqueness check: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegisterResponse {
                    success: false,
                    message: format!("Failed to verify username: {}", e),
                    user_id: None,
                }),
            );
        }
    };

    // Check if username already exists
    if all_users.iter().any(|u| u.username == payload.username) {
        info!("Registration rejected: username '{}' already exists", payload.username);
        return (
            StatusCode::CONFLICT,
            Json(RegisterResponse {
                success: false,
                message: format!("Username '{}' is already registered", payload.username),
                user_id: None,
            }),
        );
    }

    info!("Username '{}' is available, proceeding with registration", payload.username);

    // Create and register the new user
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

    // Update heartbeat timestamp + addr
    let username = payload.username.clone();
    let addr = payload.addr.clone();

    let mut online = state.online_clients.write().await;
    
    online.insert(
        username.clone(),
        OnlineClient {
            username: username.clone(),
            addr: addr.clone(),                 // store addr
            last_heartbeat: Instant::now(),
        },
    );

    info!(
        "Heartbeat received from: {} at {} (total online: {})",
        username,
        addr,
        online.len()
    );

    (
        StatusCode::OK,
        Json(HeartbeatResponse {
            success: true,
            message: format!("Heartbeat accepted for '{}' at {}", username, addr),
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
    let (is_leader, _leader_addr) = {
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

    // Return currently online clients with username + addr
    let online = state.online_clients.read().await;
    let online_list: Vec<DiscoveryClient> = online
        .values()
        .map(|client| DiscoveryClient {
            username: client.username.clone(),
            addr: client.addr.clone(),
        })
        .collect();

    info!(
        "Discovery request served: {} clients online",
        online_list.len()
    );

    (
        StatusCode::OK,
        Json(DiscoveryResponse {
            online_clients: online_list,
            count: online.len(),
            is_leader: true,
        }),
    )
}

// Upload image endpoint - ONLY LEADER CAN PROCESS
async fn upload_image(
    State(state): State<AppState>,
    axum::extract::Path(username): axum::extract::Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        return (
            StatusCode::FORBIDDEN,
            Json(ImageUploadResponse {
                success: false,
                message: format!(
                    "This node is not the leader. Current leader: {}",
                    leader_addr.unwrap_or_else(|| "unknown".to_string())
                ),
                filename: None,
            }),
        );
    }

    // Extract image data from multipart
    let mut image_data = None;
    let mut format = ImageFormat::Png; // default

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "image" {
            let content_type = field.content_type().unwrap_or("").to_string();
            format = if content_type.contains("jpeg") || content_type.contains("jpg") {
                ImageFormat::Jpeg
            } else if content_type.contains("webp") {
                ImageFormat::WebP
            } else {
                ImageFormat::Png
            };

            image_data = Some(field.bytes().await.unwrap_or_default().to_vec());
        }
    }

    let Some(data) = image_data else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ImageUploadResponse {
                success: false,
                message: "No image data provided".to_string(),
                filename: None,
            }),
        );
    };

    // Upload image
    let image_storage = ImageStorage::new(&state.user_directory);
    
    match image_storage.upload_image(&username, data, format).await {
        Ok(filename) => {
            info!("Image uploaded for user '{}': {}", username, filename);
            (
                StatusCode::CREATED,
                Json(ImageUploadResponse {
                    success: true,
                    message: format!("Image uploaded successfully"),
                    filename: Some(filename),
                }),
            )
        }
        Err(e) => {
            tracing::error!("Image upload failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ImageUploadResponse {
                    success: false,
                    message: format!("Upload failed: {}", e),
                    filename: None,
                }),
            )
        }
    }
}

// List images endpoint - ONLY LEADER CAN PROCESS
async fn list_user_images(
    State(state): State<AppState>,
    axum::extract::Path(username): axum::extract::Path<String>,
) -> impl IntoResponse {
    let (is_leader, _) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        return (
            StatusCode::FORBIDDEN,
            Json(ImageListResponse {
                images: vec![],
                count: 0,
            }),
        );
    }

    let image_storage = ImageStorage::new(&state.user_directory);
    
    match image_storage.list_images(&username).await {
        Ok(images) => {
            let count = images.len();
            (
                StatusCode::OK,
                Json(ImageListResponse { images, count }),
            )
        }
        Err(e) => {
            tracing::error!("Failed to list images: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ImageListResponse {
                    images: vec![],
                    count: 0,
                }),
            )
        }
    }
}

// Download image endpoint - ONLY LEADER CAN PROCESS
async fn download_image(
    State(state): State<AppState>,
    axum::extract::Path((username, filename)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let (is_leader, _) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        return Err((StatusCode::FORBIDDEN, "Not leader".to_string()));
    }

    let image_storage = ImageStorage::new(&state.user_directory);
    
    match image_storage.download_image(&username, &filename).await {
        Ok(data) => Ok(data),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Image not found: {}", e))),
    }
}

// Discover with images endpoint - ONLY LEADER CAN PROCESS
async fn discover_with_images(State(state): State<AppState>) -> impl IntoResponse {
    // Check if this node is the leader
    let (is_leader, leader_addr) = {
        let ns = state.node_state.read().await;
        (ns.state == crate::State::Leader, ns.leader.clone())
    };

    if !is_leader {
        info!("Discover with images request rejected - not leader");
        return (
            StatusCode::FORBIDDEN,
            Json(DiscoverWithImagesResponse {
                online_clients: vec![],
                count: 0,
            }),
        );
    }

    // Get online clients from heartbeat HashMap
    let online = state.online_clients.read().await;
    let online_usernames: Vec<(String, String)> = online
        .values()
        .map(|client| (client.username.clone(), client.addr.clone()))
        .collect();
    drop(online); // Release lock

    info!(
        "Discover with images request: {} clients online",
        online_usernames.len()
    );

    let image_storage = ImageStorage::new(&state.user_directory);
    let mut clients_with_images = Vec::new();

    // For each online client, fetch their images
    for (username, addr) in online_usernames {
        let mut images_data = Vec::new();

        // List images for this user
        match image_storage.list_images(&username).await {
            Ok(image_filenames) => {
                // Limit to 20 images per user
                let limited_filenames: Vec<_> = image_filenames.into_iter().take(20).collect();

                info!(
                    "Fetching {} images for user '{}'",
                    limited_filenames.len(),
                    username
                );

                // Download each image and base64 encode
                for filename in limited_filenames {
                    match image_storage.download_image(&username, &filename).await {
                        Ok(data) => {
                            // Base64 encode
                            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
                            images_data.push(ImageWithData {
                                filename,
                                data: encoded,
                            });
                        }
                        Err(e) => {
                            warn!("Failed to download image {}/{}: {}", username, filename, e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to list images for user '{}': {}", username, e);
                // Continue with empty images for this user
            }
        }

        clients_with_images.push(OnlineClientWithImages {
            username: username.clone(),
            addr,
            images: images_data,
        });
    }

    let count = clients_with_images.len();
    info!(
        "Discover with images response prepared: {} clients",
        count
    );

    (
        StatusCode::OK,
        Json(DiscoverWithImagesResponse {
            online_clients: clients_with_images,
            count,
        }),
    )
}
