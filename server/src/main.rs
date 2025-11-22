use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpResponse, Result as ActixResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct RegisterResponse {
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeartbeatRequest {
    username: String,
    ip: String,
    port: u16,
}

#[derive(Debug, Serialize)]
struct HeartbeatResponse {
    status: String,
    last_seen: String,
}

#[derive(Debug, Serialize)]
struct UserInfo {
    username: String,
    ip: String,
    port: u16,
}

#[derive(Debug, Serialize)]
struct DiscoveryResponse {
    online: Vec<UserInfo>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    status: String,
}

#[derive(Debug, Serialize)]
struct DebugResponse {
    users: Vec<UserDebugInfo>,
}

#[derive(Debug, Serialize)]
struct UserDebugInfo {
    username: String,
    last_seen: Option<String>,
    elapsed_seconds: Option<i64>,
    is_online: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    username: String,
    password: String,
}

#[derive(Debug, Clone)]
struct UserPresence {
    last_seen: DateTime<Utc>,
    ip: String,
    port: u16,
}

type LastSeenMap = Arc<Mutex<HashMap<String, UserPresence>>>;

struct AppState {
    last_seen: LastSeenMap,
    users_file: String,
    ttl_seconds: u64,
}

fn load_users(users_file: &str) -> Vec<User> {
    if Path::new(users_file).exists() {
        if let Ok(content) = fs::read_to_string(users_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    }
}

fn save_users(users_file: &str, users: &[User]) -> std::io::Result<()> {
    if let Some(parent) = Path::new(users_file).parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(users)?;
    fs::write(users_file, json)?;
    Ok(())
}

async fn register(
    req: web::Json<RegisterRequest>,
    state: web::Data<AppState>,
) -> ActixResult<HttpResponse> {
    let mut users = load_users(&state.users_file);
    
    // Check if username already exists
    if users.iter().any(|u| u.username == req.username) {
        return Ok(HttpResponse::BadRequest().json(ErrorResponse {
            error: "exists".to_string(),
        }));
    }
    
    // Add new user
    users.push(User {
        username: req.username.clone(),
        password: req.password.clone(),
    });
    
    if let Err(e) = save_users(&state.users_file, &users) {
        log::error!("Failed to save users: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ErrorResponse {
            error: "failed to save".to_string(),
        }));
    }
    
    log::info!("Registered user: {}", req.username);
    
    Ok(HttpResponse::Ok().json(RegisterResponse {
        status: "ok".to_string(),
    }))
}

async fn heartbeat(
    req: web::Json<HeartbeatRequest>,
    state: web::Data<AppState>,
) -> ActixResult<HttpResponse> {
    let now = Utc::now();
    let username = req.username.clone();
    let ip = req.ip.clone();
    let port = req.port;
    
    {
        let mut last_seen = state.last_seen.lock().unwrap();
        last_seen.insert(username.clone(), UserPresence {
            last_seen: now,
            ip: ip.clone(),
            port,
        });
    }
    
    log::info!("Heartbeat from: {} at {} ({}:{})", username, now.to_rfc3339(), ip, port);
    
    Ok(HttpResponse::Ok().json(HeartbeatResponse {
        status: "ok".to_string(),
        last_seen: now.to_rfc3339(),
    }))
}

async fn discovery_online(state: web::Data<AppState>) -> ActixResult<HttpResponse> {
    let now = Utc::now();
    let ttl_seconds = state.ttl_seconds as i64;
    
    let last_seen = state.last_seen.lock().unwrap();
    let online: Vec<UserInfo> = last_seen
        .iter()
        .filter_map(|(username, presence)| {
            let elapsed = now.signed_duration_since(presence.last_seen);
            let elapsed_seconds = elapsed.num_seconds();
            
            // Handle negative durations (shouldn't happen, but protect against clock skew)
            if elapsed_seconds < 0 {
                log::warn!("Negative elapsed time for {}: {} seconds (clock skew?)", username, elapsed_seconds);
                return Some(UserInfo {
                    username: username.clone(),
                    ip: presence.ip.clone(),
                    port: presence.port,
                }); // Consider online if time is in future
            }
            
            if elapsed_seconds <= ttl_seconds {
                log::debug!("User {} is online (elapsed: {}s, TTL: {}s)", username, elapsed_seconds, ttl_seconds);
                Some(UserInfo {
                    username: username.clone(),
                    ip: presence.ip.clone(),
                    port: presence.port,
                })
            } else {
                log::debug!("User {} is offline (elapsed: {}s, TTL: {}s)", username, elapsed_seconds, ttl_seconds);
                None
            }
        })
        .collect();
    
    log::info!("Discovery query: {} users online", online.len());
    Ok(HttpResponse::Ok().json(DiscoveryResponse { online }))
}

async fn status() -> ActixResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(StatusResponse {
        status: "ok".to_string(),
    }))
}

async fn debug_info(state: web::Data<AppState>) -> ActixResult<HttpResponse> {
    let now = Utc::now();
    let ttl_seconds = state.ttl_seconds as i64;
    
    let last_seen = state.last_seen.lock().unwrap();
    let users: Vec<UserDebugInfo> = last_seen
        .iter()
        .map(|(username, presence)| {
            let elapsed = now.signed_duration_since(presence.last_seen);
            let elapsed_seconds = elapsed.num_seconds();
            let is_online = elapsed_seconds >= 0 && elapsed_seconds <= ttl_seconds;
            
            UserDebugInfo {
                username: format!("{} ({}:{})", username, presence.ip, presence.port),
                last_seen: Some(presence.last_seen.to_rfc3339()),
                elapsed_seconds: Some(elapsed_seconds),
                is_online,
            }
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(DebugResponse { users }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    let ttl_seconds = std::env::var("HEARTBEAT_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    
    let users_file = "data/users.json".to_string();
    
    // Ensure data directory exists
    if let Some(parent) = Path::new(&users_file).parent() {
        fs::create_dir_all(parent)?;
    }
    
    let app_state = web::Data::new(AppState {
        last_seen: Arc::new(Mutex::new(HashMap::new())),
        users_file,
        ttl_seconds,
    });
    
    log::info!("Starting P2P Discovery Server on http://0.0.0.0:8000");
    log::info!("Heartbeat TTL: {} seconds", ttl_seconds);
    
    HttpServer::new(move || {
        let cors = Cors::permissive();
        
        App::new()
            .app_data(app_state.clone())
            .wrap(cors)
            .route("/register", web::post().to(register))
            .route("/heartbeat", web::post().to(heartbeat))
            .route("/discovery/online", web::get().to(discovery_online))
            .route("/status", web::get().to(status))
            .route("/debug", web::get().to(debug_info))
    })
    .bind(("0.0.0.0", 8000))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use chrono::Utc;
    use std::time::Duration;
    
    fn create_test_state() -> web::Data<AppState> {
        let users_file = "data/test_users.json".to_string();
        web::Data::new(AppState {
            last_seen: Arc::new(Mutex::new(HashMap::new())),
            users_file,
            ttl_seconds: 10,
        })
    }
    
    #[actix_web::test]
    async fn test_register_new_user() {
        let state = create_test_state();
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/register", web::post().to(register))
        ).await;
        
        let req = test::TestRequest::post()
            .uri("/register")
            .set_json(&RegisterRequest {
                username: "testuser".to_string(),
                password: "testpass".to_string(),
            })
            .to_request();
        
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
    
    #[actix_web::test]
    async fn test_register_duplicate_user() {
        let state = create_test_state();
        
        // Register first time
        let app1 = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/register", web::post().to(register))
        ).await;
        
        let req1 = test::TestRequest::post()
            .uri("/register")
            .set_json(&RegisterRequest {
                username: "duplicate".to_string(),
                password: "pass".to_string(),
            })
            .to_request();
        
        let resp1 = test::call_service(&app1, req1).await;
        assert!(resp1.status().is_success());
        
        // Try to register again
        let app2 = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/register", web::post().to(register))
        ).await;
        
        let req2 = test::TestRequest::post()
            .uri("/register")
            .set_json(&RegisterRequest {
                username: "duplicate".to_string(),
                password: "pass2".to_string(),
            })
            .to_request();
        
        let resp2 = test::call_service(&app2, req2).await;
        assert_eq!(resp2.status().as_u16(), 400);
    }
    
    #[actix_web::test]
    async fn test_heartbeat_updates_last_seen() {
        let state = create_test_state();
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/heartbeat", web::post().to(heartbeat))
        ).await;
        
        let username = "heartbeat_user".to_string();
        let req = test::TestRequest::post()
            .uri("/heartbeat")
            .set_json(&HeartbeatRequest {
                username: username.clone(),
                ip: "127.0.0.1".to_string(),
                port: 9000,
            })
            .to_request();
        
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        
        // Check that last_seen was updated
        let last_seen = state.last_seen.lock().unwrap();
        assert!(last_seen.contains_key(&username));
    }
    
    #[actix_web::test]
    async fn test_discovery_excludes_expired_users() {
        let state = create_test_state();
        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .route("/discovery/online", web::get().to(discovery_online))
                .route("/heartbeat", web::post().to(heartbeat))
        ).await;
        
        // Send heartbeat for user1
        let req1 = test::TestRequest::post()
            .uri("/heartbeat")
            .set_json(&HeartbeatRequest {
                username: "user1".to_string(),
                ip: "127.0.0.1".to_string(),
                port: 9001,
            })
            .to_request();
        test::call_service(&app, req1).await;
        
        // Manually set user2 to expired time
        {
            let mut last_seen = state.last_seen.lock().unwrap();
            last_seen.insert("user2".to_string(), UserPresence {
                last_seen: Utc::now() - chrono::Duration::seconds(60),
                ip: "127.0.0.1".to_string(),
                port: 9002,
            });
        }
        
        // Query discovery
        let req = test::TestRequest::get()
            .uri("/discovery/online")
            .to_request();
        
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        
        let body: DiscoveryResponse = test::read_body_json(resp).await;
        assert!(body.online.iter().any(|u| u.username == "user1"));
        assert!(!body.online.iter().any(|u| u.username == "user2"));
    }
}

