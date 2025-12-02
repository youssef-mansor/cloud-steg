use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpResponse, Result as ActixResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    sample_images: Vec<String>, // base64 encoded 128x128 images
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PhotoRequest {
    requester: String,
    owner: String,
    photo_id: String, // ID of the sample image
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PhotoApproval {
    request_id: String,
    approved: bool,
    max_views: u32, // Number of times requester can view the original
    expiry_hours: Option<u32>, // Optional expiry time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewRecord {
    request_id: String,
    requester: String,
    owner: String,
    photo_id: String,
    max_views: u32,
    current_views: u32,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PhotoRequestReq {
    owner: String,
    photo_id: String,
    message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PhotoApprovalReq {
    request_id: String,
    approved: bool,
    max_views: u32,
    expiry_hours: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ViewPhotoReq {
    request_id: String,
}

#[derive(Debug, Serialize)]
struct ViewPhotoResponse {
    success: bool,
    image_data: Option<String>, // base64 original image
    views_remaining: u32,
    message: Option<String>,
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
    sample_images: Vec<String>, // base64 encoded 128x128 images
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
    photo_requests: Arc<Mutex<HashMap<String, PhotoRequest>>>, // request_id -> request
    view_records: Arc<Mutex<HashMap<String, ViewRecord>>>, // request_id -> view record
    photo_requests_file: String,
    view_records_file: String,
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

fn load_photo_requests(file: &str) -> HashMap<String, PhotoRequest> {
    if Path::new(file).exists() {
        if let Ok(content) = fs::read_to_string(file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    }
}

fn save_photo_requests(file: &str, requests: &HashMap<String, PhotoRequest>) -> std::io::Result<()> {
    if let Some(parent) = Path::new(file).parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(requests)?;
    fs::write(file, json)?;
    Ok(())
}

fn load_view_records(file: &str) -> HashMap<String, ViewRecord> {
    if Path::new(file).exists() {
        if let Ok(content) = fs::read_to_string(file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    }
}

fn save_view_records(file: &str, records: &HashMap<String, ViewRecord>) -> std::io::Result<()> {
    if let Some(parent) = Path::new(file).parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(records)?;
    fs::write(file, json)?;
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
        sample_images: req.sample_images.clone(),
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

// Request access to someone's original photo
async fn request_photo_access(
    req: web::Json<PhotoRequestReq>,
    state: web::Data<AppState>,
    path: web::Path<String>, // requester username
) -> ActixResult<HttpResponse> {
    let requester = path.into_inner();
    let request_id = Uuid::new_v4().to_string();
    
    let photo_request = PhotoRequest {
        requester: requester.clone(),
        owner: req.owner.clone(),
        photo_id: req.photo_id.clone(),
        message: req.message.clone(),
    };
    
    // Store the request
    {
        let mut requests = state.photo_requests.lock().unwrap();
        requests.insert(request_id.clone(), photo_request);
    }
    
    // Save to file
    let requests = state.photo_requests.lock().unwrap();
    if let Err(e) = save_photo_requests(&state.photo_requests_file, &requests) {
        log::error!("Failed to save photo requests: {}", e);
    }
    
    log::info!("Photo access request: {} -> {} (photo: {})", requester, req.owner, req.photo_id);
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "request_id": request_id,
        "message": "Photo access request sent"
    })))
}

// Approve or deny a photo request
async fn approve_photo_request(
    req: web::Json<PhotoApprovalReq>,
    state: web::Data<AppState>,
    path: web::Path<String>, // owner username
) -> ActixResult<HttpResponse> {
    let owner = path.into_inner();
    let request_id = req.request_id.clone();
    
    // Find the original request
    let photo_request = {
        let requests = state.photo_requests.lock().unwrap();
        match requests.get(&request_id) {
            Some(request) if request.owner == owner => request.clone(),
            Some(_) => {
                return Ok(HttpResponse::Forbidden().json(serde_json::json!({
                    "error": "Not authorized to approve this request"
                })));
            }
            None => {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Request not found"
                })));
            }
        }
    };
    
    if req.approved {
        // Create view record
        let now = Utc::now();
        let expires_at = req.expiry_hours.map(|hours| now + chrono::Duration::hours(hours as i64));
        
        let view_record = ViewRecord {
            request_id: request_id.clone(),
            requester: photo_request.requester.clone(),
            owner: owner.clone(),
            photo_id: photo_request.photo_id.clone(),
            max_views: req.max_views,
            current_views: 0,
            created_at: now,
            expires_at,
        };
        
        // Store the view record
        {
            let mut records = state.view_records.lock().unwrap();
            records.insert(request_id.clone(), view_record);
        }
        
        // Save to file
        let records = state.view_records.lock().unwrap();
        if let Err(e) = save_view_records(&state.view_records_file, &records) {
            log::error!("Failed to save view records: {}", e);
        }
        
        log::info!("Photo request approved: {} can view {}'s photo {} times", 
                  photo_request.requester, owner, req.max_views);
        
        Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "approved",
            "message": format!("Access granted for {} views", req.max_views)
        })))
    } else {
        // Remove the request
        {
            let mut requests = state.photo_requests.lock().unwrap();
            requests.remove(&request_id);
        }
        
        log::info!("Photo request denied: {} -> {}", photo_request.requester, owner);
        
        Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "denied",
            "message": "Access denied"
        })))
    }
}

// View an original photo (consumes one view count)
async fn view_photo(
    req: web::Json<ViewPhotoReq>,
    state: web::Data<AppState>,
    path: web::Path<String>, // requester username
) -> ActixResult<HttpResponse> {
    let requester = path.into_inner();
    let request_id = req.request_id.clone();
    
    let mut view_record = {
        let mut records = state.view_records.lock().unwrap();
        match records.get_mut(&request_id) {
            Some(record) if record.requester == requester => {
                // Check if expired
                if let Some(expires_at) = record.expires_at {
                    if Utc::now() > expires_at {
                        records.remove(&request_id);
                        return Ok(HttpResponse::Gone().json(ViewPhotoResponse {
                            success: false,
                            image_data: None,
                            views_remaining: 0,
                            message: Some("Access expired".to_string()),
                        }));
                    }
                }
                
                // Check if views exhausted
                if record.current_views >= record.max_views {
                    return Ok(HttpResponse::Forbidden().json(ViewPhotoResponse {
                        success: false,
                        image_data: None,
                        views_remaining: 0,
                        message: Some("View limit exceeded".to_string()),
                    }));
                }
                
                // Increment view count
                record.current_views += 1;
                let views_remaining = record.max_views - record.current_views;
                record.clone()
            }
            Some(_) => {
                return Ok(HttpResponse::Forbidden().json(ViewPhotoResponse {
                    success: false,
                    image_data: None,
                    views_remaining: 0,
                    message: Some("Not authorized".to_string()),
                }));
            }
            None => {
                return Ok(HttpResponse::NotFound().json(ViewPhotoResponse {
                    success: false,
                    image_data: None,
                    views_remaining: 0,
                    message: Some("Request not found".to_string()),
                }));
            }
        }
    };
    
    // Get the owner's full resolution image
    let users = load_users(&state.users_file);
    let owner_user = users.iter().find(|u| u.username == view_record.owner);
    
    match owner_user {
        Some(user) => {
            // Find the specific image by photo_id (assuming photo_id is index)
            let photo_index: usize = view_record.photo_id.parse().unwrap_or(0);
            let image_data = user.sample_images.get(photo_index).cloned();
            
            if let Some(image) = image_data {
                // Save updated view record
                let records = state.view_records.lock().unwrap();
                if let Err(e) = save_view_records(&state.view_records_file, &records) {
                    log::error!("Failed to save view records: {}", e);
                }
                
                log::info!("Photo viewed: {} viewed {}'s photo (views: {}/{})", 
                          requester, view_record.owner, view_record.current_views, view_record.max_views);
                
                Ok(HttpResponse::Ok().json(ViewPhotoResponse {
                    success: true,
                    image_data: Some(image),
                    views_remaining: view_record.max_views - view_record.current_views,
                    message: None,
                }))
            } else {
                Ok(HttpResponse::NotFound().json(ViewPhotoResponse {
                    success: false,
                    image_data: None,
                    views_remaining: view_record.max_views - view_record.current_views,
                    message: Some("Photo not found".to_string()),
                }))
            }
        }
        None => {
            Ok(HttpResponse::NotFound().json(ViewPhotoResponse {
                success: false,
                image_data: None,
                views_remaining: 0,
                message: Some("Owner not found".to_string()),
            }))
        }
    }
}

// List photo requests (for owner to see pending requests)
async fn list_photo_requests(
    state: web::Data<AppState>,
    path: web::Path<String>, // username
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    
    let requests = state.photo_requests.lock().unwrap();
    let user_requests: Vec<&PhotoRequest> = requests
        .values()
        .filter(|req| req.owner == username)
        .collect();
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "requests": user_requests
    })))
}

// List access records (for requester to see their current permissions)
async fn list_access_records(
    state: web::Data<AppState>,
    path: web::Path<String>, // username
) -> ActixResult<HttpResponse> {
    let username = path.into_inner();
    
    let records = state.view_records.lock().unwrap();
    let user_records: Vec<serde_json::Value> = records
        .values()
        .filter(|record| record.requester == username)
        .map(|record| {
            let now = Utc::now();
            let is_expired = record.expires_at.map_or(false, |exp| now > exp);
            let views_remaining = if record.current_views >= record.max_views {
                0
            } else {
                record.max_views - record.current_views
            };
            
            serde_json::json!({
                "request_id": record.request_id,
                "owner": record.owner,
                "photo_id": record.photo_id,
                "max_views": record.max_views,
                "current_views": record.current_views,
                "views_remaining": views_remaining,
                "created_at": record.created_at,
                "expires_at": record.expires_at,
                "is_expired": is_expired,
                "can_view": !is_expired && views_remaining > 0
            })
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "access_records": user_records
    })))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    let ttl_seconds = std::env::var("HEARTBEAT_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    
    let users_file = "data/users.json".to_string();
    let photo_requests_file = "data/photo_requests.json".to_string();
    let view_records_file = "data/view_records.json".to_string();
    
    // Ensure data directory exists
    if let Some(parent) = Path::new(&users_file).parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Load existing data
    let photo_requests = load_photo_requests(&photo_requests_file);
    let view_records = load_view_records(&view_records_file);
    
    let app_state = web::Data::new(AppState {
        last_seen: Arc::new(Mutex::new(HashMap::new())),
        users_file,
        ttl_seconds,
        photo_requests: Arc::new(Mutex::new(photo_requests)),
        view_records: Arc::new(Mutex::new(view_records)),
        photo_requests_file,
        view_records_file,
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
            // Photo sharing endpoints
            .route("/photo/request/{requester}", web::post().to(request_photo_access))
            .route("/photo/approve/{owner}", web::post().to(approve_photo_request))
            .route("/photo/view/{requester}", web::post().to(view_photo))
            .route("/photo/requests/{username}", web::get().to(list_photo_requests))
            .route("/photo/access/{username}", web::get().to(list_access_records))
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
        let photo_requests_file = "data/test_photo_requests.json".to_string();
        let view_records_file = "data/test_view_records.json".to_string();
        web::Data::new(AppState {
            last_seen: Arc::new(Mutex::new(HashMap::new())),
            users_file,
            ttl_seconds: 10,
            photo_requests: Arc::new(Mutex::new(HashMap::new())),
            view_records: Arc::new(Mutex::new(HashMap::new())),
            photo_requests_file,
            view_records_file,
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
                sample_images: vec!["base64image".to_string()],
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
                sample_images: vec!["base64image1".to_string()],
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
                sample_images: vec!["base64image2".to_string()],
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

