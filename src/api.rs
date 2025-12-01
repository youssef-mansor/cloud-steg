//! HTTP API for user registration

use crate::registration::{UserDirectory, UserInfo};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub user_directory: Arc<UserDirectory>,
}

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub success: bool,
    pub message: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
    pub count: usize,
}

// Configure routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route("/register", post(register_user))
        .route("/users", get(list_users))
        .with_state(state)
}

// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "distributed-system-registration"
    }))
}

// Register endpoint
async fn register_user(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    info!("Registration request for username: {}", payload.username);

    let user = UserInfo::new(payload.username, payload.email);

    match state.user_directory.register_user(&user).await {
        Ok(_) => {
            info!("Successfully registered user: {}", user.username);
            (
                StatusCode::CREATED,
                Json(RegisterResponse {
                    success: true,
                    message: format!("User '{}' registered successfully", user.username),
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

// List users endpoint
async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
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
