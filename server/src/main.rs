use actix_cors::Cors;
use actix_web::{web, App, HttpServer, HttpResponse, Result as ActixResult};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::time::{Duration as StdDuration, Instant};
use tokio::sync::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;
use uuid::Uuid;
use clap::Parser;
use anyhow::Context;
use sysinfo::{CpuExt, System, SystemExt};
use rand::Rng;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    ip: String,
    port: u16,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_images: Option<Vec<String>>, // base64 encoded 128x128 images
}

#[derive(Debug, Serialize)]
struct DiscoveryResponse {
    online: Vec<UserInfo>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    status: String,
    is_leader: Option<bool>,
    current_leader: Option<String>,
}

// Leader Election Types
#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value = "config.toml")]
    config: Option<String>,

    #[clap(long)]
    this_node: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct ElectionConfig {
    this_node: String,
    peers: Vec<String>,
    heartbeat_interval_ms: u64,
    election_timeout_min_ms: u64,
    election_timeout_max_ms: u64,
    leader_term_ms: u64,
    net_timeout_ms: u64,
    cpu_refresh_ms: u64,
    election_retry_ms: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum Message {
    Heartbeat { leader: String, term_end_unix: u64, term: u64 },
    GetCpu { term: u64, initiator_addr: String, initiator_cpu: f32 },
    CpuResp { cpu_percent: f32, addr: String, term: u64 },
    LeaderAnnounce { leader: String, term_end_unix: u64, term: u64 },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Follower,
    Leader,
}

#[derive(Debug)]
pub struct NodeState {
    state: State,
    leader: Option<String>,
    last_heartbeat: Option<Instant>,
    term_end: Option<Instant>,
    startup_time: Instant,
    current_term: u64,
    cpu_snapshot: f32,
}

fn random_election_timeout(cfg: &ElectionConfig) -> u64 {
    rand::thread_rng().gen_range(cfg.election_timeout_min_ms..=cfg.election_timeout_max_ms)
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
    ip: String,
    port: u16,
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
    node_state: Option<Arc<RwLock<NodeState>>>, // Leader election state (None if not using leader election)
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

// Helper function to check if this node is the leader
async fn is_leader(state: &web::Data<AppState>) -> (bool, Option<String>) {
    if let Some(ref node_state) = state.node_state {
        let ns = node_state.read().await;
        (ns.state == State::Leader, ns.leader.clone())
    } else {
        // If no leader election, always return true (single server mode)
        (true, None)
    }
}

async fn register(
    req: web::Json<RegisterRequest>,
    state: web::Data<AppState>,
) -> ActixResult<HttpResponse> {
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            error: format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string())),
        }));
    }
    
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
        ip: req.ip.clone(),
        port: req.port,
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
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            error: format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string())),
        }));
    }
    
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
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(ErrorResponse {
            error: format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string())),
        }));
    }
    
    let now = Utc::now();
    let ttl_seconds = state.ttl_seconds as i64;
    
    // Load users to get their images
    let users = load_users(&state.users_file);
    let users_map: HashMap<String, &User> = users.iter().map(|u| (u.username.clone(), u)).collect();
    
    let last_seen = state.last_seen.lock().unwrap();
    let online: Vec<UserInfo> = last_seen
        .iter()
        .filter_map(|(username, presence)| {
            let elapsed = now.signed_duration_since(presence.last_seen);
            let elapsed_seconds = elapsed.num_seconds();
            
            // Handle negative durations (shouldn't happen, but protect against clock skew)
            if elapsed_seconds < 0 {
                log::warn!("Negative elapsed time for {}: {} seconds (clock skew?)", username, elapsed_seconds);
                let user_info = if let Some(user) = users_map.get(username) {
                    UserInfo {
                        username: username.clone(),
                        ip: presence.ip.clone(),
                        port: presence.port,
                        sample_images: Some(user.sample_images.clone()),
                    }
                } else {
                    UserInfo {
                        username: username.clone(),
                        ip: presence.ip.clone(),
                        port: presence.port,
                        sample_images: None,
                    }
                };
                return Some(user_info);
            }
            
            if elapsed_seconds <= ttl_seconds {
                log::debug!("User {} is online (elapsed: {}s, TTL: {}s)", username, elapsed_seconds, ttl_seconds);
                let user_info = if let Some(user) = users_map.get(username) {
                    UserInfo {
                        username: username.clone(),
                        ip: presence.ip.clone(),
                        port: presence.port,
                        sample_images: Some(user.sample_images.clone()),
                    }
                } else {
                    UserInfo {
                        username: username.clone(),
                        ip: presence.ip.clone(),
                        port: presence.port,
                        sample_images: None,
                    }
                };
                Some(user_info)
            } else {
                log::debug!("User {} is offline (elapsed: {}s, TTL: {}s)", username, elapsed_seconds, ttl_seconds);
                None
            }
        })
        .collect();
    
    log::info!("Discovery query: {} users online", online.len());
    Ok(HttpResponse::Ok().json(DiscoveryResponse { online }))
}

async fn status(state: web::Data<AppState>) -> ActixResult<HttpResponse> {
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    Ok(HttpResponse::Ok().json(StatusResponse {
        status: "ok".to_string(),
        is_leader: Some(is_leader_node),
        current_leader: leader_addr,
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
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string()))
        })));
    }
    
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
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "error": format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string()))
        })));
    }
    
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
    // Check if this node is the leader
    let (is_leader_node, leader_addr) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(ViewPhotoResponse {
            success: false,
            image_data: None,
            views_remaining: 0,
            message: Some(format!("This node is not the leader. Current leader: {}", 
                leader_addr.unwrap_or_else(|| "unknown".to_string()))),
        }));
    }
    
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
    // Check if this node is the leader
    let (is_leader_node, _) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "requests": []
        })));
    }
    
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
    // Check if this node is the leader
    let (is_leader_node, _) = is_leader(&state).await;
    if !is_leader_node {
        return Ok(HttpResponse::Forbidden().json(serde_json::json!({
            "access_records": []
        })));
    }
    
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

// ========================================
// LEADER ELECTION FUNCTIONS
// ========================================

async fn handle_connection(
    mut stream: TcpStream,
    shared: Arc<RwLock<NodeState>>,
    cpu: Arc<RwLock<f32>>,
    this_node: String,
) -> anyhow::Result<()> {
    let peer = stream.peer_addr()?;
    let (r, mut w) = stream.split();
    let mut reader = BufReader::new(r);
    let mut buf = String::new();
    let n = reader.read_line(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let msg: Message = serde_json::from_str(buf.trim()).context("parse incoming json")?;
    match msg {
        Message::Heartbeat { leader, term_end_unix, term } => {
            let mut ns = shared.write().await;
            
            if term >= ns.current_term {
                if term > ns.current_term {
                    ns.current_term = term;
                    if ns.state == State::Leader {
                        log::info!("Stepping down: received heartbeat from higher term {}", term);
                        ns.state = State::Follower;
                    }
                }
                
                ns.last_heartbeat = Some(Instant::now());
                ns.leader = Some(leader.clone());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(0));

                let now_unix = Utc::now().timestamp() as u64;
                if term_end_unix > now_unix {
                    let remaining = term_end_unix - now_unix;
                    ns.term_end = Some(Instant::now() + StdDuration::from_secs(remaining));
                }
            } else {
                log::debug!("Rejected heartbeat from term {} (current term: {})", term, ns.current_term);
            }

            let resp = Message::Ping;
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::GetCpu { term, initiator_addr: _, initiator_cpu: _ } => {
            let snapshot_val = {
                let mut ns = shared.write().await;
                
                if term > ns.current_term {
                    ns.current_term = term;
                    ns.cpu_snapshot = *cpu.read().await;
                }
                
                ns.cpu_snapshot
            };
            
            let resp = Message::CpuResp { cpu_percent: snapshot_val, addr: peer.to_string(), term };
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::LeaderAnnounce { leader, term_end_unix, term } => {
            let mut ns = shared.write().await;

            if term >= ns.current_term {
                if term > ns.current_term {
                    ns.current_term = term;
                    if ns.state == State::Leader {
                        log::info!("[LEADER_ANNOUNCE] Stepping down: received leader announce from higher term {}", term);
                        ns.state = State::Follower;
                    }
                }

                let is_self = leader == this_node;

                if is_self {
                    log::info!("[LEADER_ANNOUNCE] I ({}) am elected leader for term {}", leader, term);
                    ns.state = State::Leader;
                    ns.leader = Some(this_node.clone());
                } else {
                    log::info!("[LEADER_ANNOUNCE] New leader {} for term {} (I become follower)", leader, term);
                    ns.state = State::Follower;
                    ns.leader = Some(leader.clone());
                }

                let now_unix = Utc::now().timestamp() as u64;
                if term_end_unix > now_unix {
                    let remaining = term_end_unix - now_unix;
                    ns.term_end = Some(Instant::now() + StdDuration::from_secs(remaining));
                } else {
                    ns.term_end = None;
                }
                ns.last_heartbeat = Some(Instant::now());
            } else {
                log::debug!("[LEADER_ANNOUNCE] Rejected leader announce from term {} (current term: {})", term, ns.current_term);
            }

            let resp = Message::Ping;
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::CpuResp { .. } => {}
        Message::Ping => {
            let resp = Message::Ping;
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
    }
    Ok(())
}

async fn run_election(
    peers: &[SocketAddr],
    this_addr_str: &str,
    cfg: &ElectionConfig,
    shared: Arc<RwLock<NodeState>>,
    cpu: Arc<RwLock<f32>>,
) -> anyhow::Result<()> {
    let (election_term, self_cpu_snapshot) = {
        let mut ns = shared.write().await;
        ns.current_term += 1;
        ns.cpu_snapshot = *cpu.read().await;
        (ns.current_term, ns.cpu_snapshot)
    };
    
    log::info!("Starting election from {} for term {} with CPU snapshot: {}%", 
             this_addr_str, election_term, self_cpu_snapshot);
    
    let mut collected: HashMap<String, f32> = HashMap::new();
    collected.insert(this_addr_str.to_string(), self_cpu_snapshot);

    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == this_addr_str {
            continue;
        }
        match request_cpu(p, cfg.net_timeout_ms, election_term, this_addr_str, self_cpu_snapshot).await {
            Ok(val) => {
                collected.insert(p.to_string(), val);
            }
            Err(e) => {
                log::warn!("failed to get cpu from {}: {}", p, e);
            }
        }
        sleep(StdDuration::from_millis(cfg.election_retry_ms)).await;
    }

    let mut chosen = None;
    for (addr, cpu_val) in collected.iter() {
        match &chosen {
            None => chosen = Some((addr.clone(), *cpu_val)),
            Some((caddr, cval)) => {
                if *cpu_val < *cval || (*cpu_val == *cval && addr < caddr) {
                    chosen = Some((addr.clone(), *cpu_val));
                }
            }
        }
    }

    if let Some((leader_addr, _)) = chosen {
        log::info!("Election result: leader -> {} (term {})", leader_addr, election_term);
        let term_end_unix =
            (Utc::now() + ChronoDuration::milliseconds(cfg.leader_term_ms as i64)).timestamp() as u64;

        if leader_addr == this_addr_str {
            {
                let mut ns = shared.write().await;
                ns.state = State::Leader;
                ns.leader = Some(this_addr_str.to_string());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(cfg.leader_term_ms));
                ns.last_heartbeat = Some(Instant::now());
            }
            log::info!("[ELECTION] I ({}) won term {}. Broadcasting LeaderAnnounce to peers", this_addr_str, election_term);
            broadcast_leader(&peers, &this_addr_str, term_end_unix, election_term, cfg.net_timeout_ms).await;
        } else {
            {
                let mut ns = shared.write().await;
                ns.state = State::Follower;
                ns.leader = Some(leader_addr.clone());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(cfg.leader_term_ms));
                ns.last_heartbeat = Some(Instant::now());
            }
            log::info!("[ELECTION] {} won term {} (I am {}). Broadcasting LeaderAnnounce", leader_addr, election_term, this_addr_str);
            broadcast_leader(&peers, &leader_addr, term_end_unix, election_term, cfg.net_timeout_ms).await;
        }
    }

    Ok(())
}

async fn request_cpu(peer: &SocketAddr, timeout_ms: u64, term: u64, initiator_addr: &str, initiator_cpu: f32) -> anyhow::Result<f32> {
    let addr = peer.to_string();
    log::debug!("[CPU Request] Connecting to {}", addr);
    let connect =
        tokio::time::timeout(StdDuration::from_millis(timeout_ms), TcpStream::connect(peer)).await;
    let mut stream = match connect {
        Ok(Ok(s)) => {
            log::debug!("[CPU Request] Connected to {}", addr);
            s
        }
        _ => {
            anyhow::bail!("connect timeout or failed to {}", addr)
        }
    };

    let msg = Message::GetCpu {  
        term, 
        initiator_addr: initiator_addr.to_string(),
        initiator_cpu 
    };
    let s = serde_json::to_string(&msg)? + "\n";
    stream.write_all(s.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    let n = tokio::time::timeout(StdDuration::from_millis(timeout_ms), reader.read_line(&mut buf))
        .await??;

    if n == 0 {
        anyhow::bail!("no response from {}", addr);
    }

    let resp: Message = serde_json::from_str(buf.trim())?;
    if let Message::CpuResp { cpu_percent, term: _, .. } = resp {
        log::debug!("[CPU Request] Received CPU {}% from {}", cpu_percent, addr);
        Ok(cpu_percent)
    }
    else {
        anyhow::bail!("unexpected response from {}", addr);
    }
}

async fn broadcast_leader(
    peers: &[SocketAddr],
    leader: &str,
    term_end_unix: u64,
    term: u64,
    timeout_ms: u64,
) {
    for p in peers.iter() {
        let p_s = p.to_string();
        log::debug!("[BROADCAST] Announcing leader {} for term {} to {}", leader, term, p_s);
        let leader_s = leader.to_string();
        let msg = Message::LeaderAnnounce {
            leader: leader_s.clone(),
            term_end_unix,
            term,
        };
        let _ = send_message(p, &msg, timeout_ms).await;
    }
}

async fn send_heartbeat_to_peers(
    peers: &[SocketAddr],
    leader: &str,
    cfg: &ElectionConfig,
    shared: Arc<RwLock<NodeState>>,
) {
    let (term_end_unix, current_term) = {
        let ns = shared.read().await;
        let term_end = (Utc::now() + ChronoDuration::milliseconds(cfg.leader_term_ms as i64)).timestamp() as u64;
        (term_end, ns.current_term)
    };
    
    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == leader {
            continue;
        }
        let msg = Message::Heartbeat { leader: leader.to_string(), term_end_unix, term: current_term };
        let _ = send_message(p, &msg, cfg.net_timeout_ms).await;
    }
}

async fn send_message(peer: &SocketAddr, msg: &Message, timeout_ms: u64) -> anyhow::Result<()> {
    let addr = peer.to_string();
    let connect =
        tokio::time::timeout(StdDuration::from_millis(timeout_ms), TcpStream::connect(peer)).await;

    let mut stream = match connect {
        Ok(Ok(s)) => s,
        _ => {
            return Err(anyhow::anyhow!("connect timeout or failed to {}", addr));
        }
    };

    let s = serde_json::to_string(msg)? + "\n";
    stream.write_all(s.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    let _ = tokio::time::timeout(StdDuration::from_millis(timeout_ms), reader.read_line(&mut buf)).await;

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    // Parse command line arguments
    let args = Args::parse();
    
    let ttl_seconds = std::env::var("HEARTBEAT_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    
    // Get port from environment or default to 8000
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8000);
    
    // Get data directory from environment or use default
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "data".to_string());
    
    let users_file = format!("{}/users.json", data_dir);
    let photo_requests_file = format!("{}/photo_requests.json", data_dir);
    let view_records_file = format!("{}/view_records.json", data_dir);
    
    // Ensure data directory exists
    if let Some(parent) = Path::new(&users_file).parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Load existing data
    let photo_requests = load_photo_requests(&photo_requests_file);
    let view_records = load_view_records(&view_records_file);
    
    // Initialize leader election if config provided
    let node_state = if let Some(config_path) = args.config {
        log::info!("Loading leader election config from: {}", config_path);
        // Check if config file exists
        if !Path::new(&config_path).exists() {
            log::warn!("Config file {} not found. Running in single-server mode.", config_path);
            None
        } else {
            match initialize_leader_election(&config_path, args.this_node).await {
                Ok((ns, peers, cfg, this_addr_str)) => {
                log::info!("✓ Leader election initialized");
                
                // Start TCP listener for leader election
                let listener = TcpListener::bind(&this_addr_str).await?;
                log::info!("✓ Leader election TCP listener bound to {}", this_addr_str);
                
                let listener_shared = ns.clone();
                let cpu_for_handler = Arc::new(RwLock::new(0f32));
                let cpu_clone = cpu_for_handler.clone();
                let cpu_for_tcp = cpu_for_handler.clone();
                let cpu_for_election = cpu_for_handler.clone();
                
                // Start CPU monitoring
                let cpu_refresh = cfg.cpu_refresh_ms;
                tokio::spawn(async move {
                    let mut sys = System::new_all();
                    loop {
                        sys.refresh_cpu();
                        let avg = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
                            / (sys.cpus().len() as f32);
                        {
                            let mut w = cpu_clone.write().await;
                            *w = avg;
                        }
                        sleep(StdDuration::from_millis(cpu_refresh)).await;
                    }
                });
                
                // Handle incoming TCP connections
                let this_node_clone = this_addr_str.clone();
                tokio::spawn(async move {
                    loop {
                        match listener.accept().await {
                            Ok((stream, addr)) => {
                                let s = listener_shared.clone();
                                let c = cpu_for_tcp.clone();
                                let this_node = this_node_clone.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(stream, s, c, this_node).await {
                                        log::warn!("handler error from {}: {}", addr, e);
                                    }
                                });
                            }
                            Err(e) => {
                                log::error!("accept error: {}", e);
                            }
                        }
                    }
                });
                
                // Start election timeout checker
                let shared_clone = ns.clone();
                let peers_clone = peers.clone();
                let cfg_clone = cfg.clone();
                let this_addr_str_clone = this_addr_str.clone();
                tokio::spawn(async move {
                    let mut election_timeout = random_election_timeout(&cfg_clone);
                    
                    loop {
                        {
                            let ns = shared_clone.read().await;
                            if ns.state == State::Follower {
                                let should_elect = if let Some(last) = ns.last_heartbeat {
                                    last.elapsed().as_millis() as u64 >= election_timeout
                                } else {
                                    ns.startup_time.elapsed().as_millis() as u64 >= election_timeout
                                };
                                
                                if should_elect {
                                    drop(ns);
                                    if let Err(e) =
                                        run_election(&peers_clone, &this_addr_str_clone, &cfg_clone, shared_clone.clone(), cpu_for_election.clone()).await
                                    {
                                        log::error!("election failed: {}", e);
                                    }
                                    election_timeout = random_election_timeout(&cfg_clone);
                                }
                            } else if ns.state == State::Leader {
                                election_timeout = random_election_timeout(&cfg_clone);
                            }
                        }
                        sleep(StdDuration::from_millis(500)).await;
                    }
                });
                
                // Start leader heartbeat sender
                let shared_clone2 = ns.clone();
                let peers_clone2 = peers.clone();
                let cfg_clone2 = cfg.clone();
                let this_addr_str_clone2 = this_addr_str.clone();
                tokio::spawn(async move {
                    loop {
                        let is_leader = {
                            let ns = shared_clone2.read().await;
                            ns.state == State::Leader
                        };
                        if is_leader {
                            send_heartbeat_to_peers(&peers_clone2, &this_addr_str_clone2, &cfg_clone2, shared_clone2.clone()).await;

                            let end_reached = {
                                let ns = shared_clone2.read().await;
                                if let Some(end) = ns.term_end {
                                    Instant::now() >= end
                                } else {
                                    false
                                }
                            };

                            if end_reached {
                                {
                                    let mut ns = shared_clone2.write().await;
                                    ns.state = State::Follower;
                                    ns.leader = None;
                                    ns.term_end = None;
                                    ns.last_heartbeat = None;
                                }
                                sleep(StdDuration::from_millis(200)).await;
                            }
                        }
                        sleep(StdDuration::from_millis(cfg_clone2.heartbeat_interval_ms)).await;
                    }
                });
                
                    Some(ns)
                }
                Err(e) => {
                    log::warn!("Failed to initialize leader election: {}. Running in single-server mode.", e);
                    None
                }
            }
        }
    } else {
        log::info!("No leader election config provided. Running in single-server mode.");
        None
    };
    
    let app_state = web::Data::new(AppState {
        last_seen: Arc::new(Mutex::new(HashMap::new())),
        users_file,
        ttl_seconds,
        photo_requests: Arc::new(Mutex::new(photo_requests)),
        view_records: Arc::new(Mutex::new(view_records)),
        photo_requests_file,
        view_records_file,
        node_state,
    });
    
    log::info!("Starting P2P Discovery Server on http://0.0.0.0:{}", port);
    log::info!("Heartbeat TTL: {} seconds", ttl_seconds);
    log::info!("Data directory: {}", data_dir);
    if app_state.node_state.is_some() {
        log::info!("Leader election: ENABLED");
    } else {
        log::info!("Leader election: DISABLED (single-server mode)");
    }
    
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
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

async fn initialize_leader_election(
    config_path: &str,
    this_node_override: Option<String>,
) -> anyhow::Result<(Arc<RwLock<NodeState>>, Vec<SocketAddr>, ElectionConfig, String)> {
    let cfg_text = fs::read_to_string(config_path).context("read config")?;
    let mut cfg: ElectionConfig = toml::from_str(&cfg_text).context("parse config")?;
    
    // Override this_node if provided
    if let Some(node) = this_node_override {
        cfg.this_node = node;
    }
    
    let this_node_str = cfg.this_node.clone(); // Clone before moving cfg
    let this_addr: SocketAddr = this_node_str.parse().context("parse this_node as SocketAddr")?;
    
    let peers: Vec<SocketAddr> = cfg
        .peers
        .iter()
        .map(|s| s.parse().expect("invalid peer addr in config"))
        .collect();
    
    let node_state = Arc::new(RwLock::new(NodeState {
        state: State::Follower,
        leader: None,
        last_heartbeat: None,
        term_end: None,
        startup_time: Instant::now(),
        current_term: 0,
        cpu_snapshot: 0.0,
    }));
    
    Ok((node_state, peers, cfg, this_node_str))
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
            node_state: None, // No leader election in tests
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
                ip: "127.0.0.1".to_string(),
                port: 9000,
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
                ip: "127.0.0.1".to_string(),
                port: 9001,
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
                ip: "127.0.0.1".to_string(),
                port: 9002,
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

