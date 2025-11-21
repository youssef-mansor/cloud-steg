use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use sysinfo::{CpuExt, System, SystemExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::sleep;
use chrono::Utc;
use std::time::Duration as StdDuration;
use chrono::Duration as ChronoDuration;

#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value = "config.toml")]
    config: String,

    #[clap(long)]
    this_node: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Config {
    this_node: String,
    peers: Vec<String>,
    heartbeat_interval_secs: u64,
    heartbeat_timeout_secs: u64,
    leader_term_secs: u64,
    net_timeout_ms: u64,
    cpu_refresh_ms: u64,
    election_retry_ms: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum Message {
    Heartbeat { leader: String, term_end_unix: u64, election_term: u64 },
    GetCpu,
    CpuResp { cpu_percent: f32, addr: String },
    LeaderAnnounce { leader: String, term_end_unix: u64, election_term: u64 },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Follower,
    Leader,
}

#[derive(Debug)]
struct NodeState {
    state: State,
    leader: Option<String>,
    last_heartbeat: Option<Instant>,
    term_end: Option<Instant>,
    election_term: u64,  // NEW: Track current election term
    election_in_progress: bool,  // NEW: Prevent concurrent elections
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let cfg_text = fs::read_to_string(&args.config).context("read config")?;
    let mut cfg: Config = toml::from_str(&cfg_text).context("parse config")?;

    // override this_node if provided
    if let Some(node) = args.this_node {
        cfg.this_node = node;
    }

    let this_addr: SocketAddr = cfg.this_node.parse().context("parse this_node as SocketAddr")?;

    println!("Starting node {}", this_addr);

    let peers: Vec<SocketAddr> = cfg
        .peers
        .iter()
        .map(|s| s.parse().expect("invalid peer addr in config"))
        .collect();

    let shared = Arc::new(RwLock::new(NodeState {
        state: State::Follower,
        leader: None,
        last_heartbeat: None,
        term_end: None,
        election_term: 0,
        election_in_progress: false,
    }));

    let cpu = Arc::new(RwLock::new(0f32));
    let cpu_clone = cpu.clone();
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

    let listener = TcpListener::bind(this_addr).await?;
    let listener_shared = shared.clone();
    let cpu_for_handler = cpu.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let s = listener_shared.clone();
                    let c = cpu_for_handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, s, c).await {
                            eprintln!("handler error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("accept error: {}", e);
                }
            }
        }
    });

    let shared_clone = shared.clone();
    let peers_clone = peers.clone();
    let cfg_clone = cfg.clone();
    let this_addr_str = cfg.this_node.clone();
    tokio::spawn(async move {
        loop {
            {
                let mut ns = shared_clone.write().await;
                if ns.state == State::Follower && !ns.election_in_progress {
                    let should_elect = if let Some(last) = ns.last_heartbeat {
                        last.elapsed().as_secs() >= cfg_clone.heartbeat_timeout_secs
                    } else {
                        true
                    };
                    if should_elect {
                        println!("[ELECTION] Timeout detected, starting election...");
                        ns.election_in_progress = true;
                        drop(ns);
                        
                        if let Err(e) =
                            run_election(&peers_clone, &this_addr_str, &cfg_clone, shared_clone.clone(), cpu.clone()).await
                        {
                            eprintln!("[ELECTION] Election failed: {}", e);
                            let mut ns = shared_clone.write().await;
                            ns.election_in_progress = false;
                        }
                    }
                }
            }
            sleep(StdDuration::from_millis(500)).await;
        }
    });

    let shared_clone2 = shared.clone();
    let peers_clone2 = peers.clone();
    let cfg_clone2 = cfg.clone();
    let this_addr_str2 = cfg.this_node.clone();
    tokio::spawn(async move {
        loop {
            let is_leader = {
                let ns = shared_clone2.read().await;
                ns.state == State::Leader
            };
            if is_leader {
                send_heartbeat_to_peers(&peers_clone2, &this_addr_str2, &cfg_clone2, shared_clone2.clone()).await;

                let end_reached = {
                    let ns = shared_clone2.read().await;
                    if let Some(end) = ns.term_end {
                        Instant::now() >= end
                    } else {
                        false
                    }
                };

                if end_reached {
                    // Increment term and prepare to broadcast stepping down
                    let new_term = {
                        let mut ns = shared_clone2.write().await;
                        ns.state = State::Follower;
                        ns.leader = None;
                        ns.term_end = None;
                        ns.last_heartbeat = None;
                        ns.election_term += 1;  // Increment term when stepping down
                        println!("[TERM] Leader term expired, stepping down to Term {}", ns.election_term);
                        ns.election_term
                    };
                    
                    log_state_change(&shared_clone2, &this_addr_str2).await;
                    
                    // FIX 1: Broadcast term expiration to all followers
                    // This forces followers to clear their leader state and prepare for new election
                    println!("[TERM] Broadcasting term expiration (Term {}) to all peers", new_term);
                    for p in peers_clone2.iter() {
                        let p_s = p.to_string();
                        if p_s == this_addr_str2 {
                            continue;
                        }
                        // Send LeaderAnnounce with empty leader string to signal term end
                        let msg = Message::LeaderAnnounce { 
                            leader: String::new(),  // Empty leader signals stepping down
                            term_end_unix: 0,       // 0 indicates no active term
                            election_term: new_term 
                        };
                        println!("[TERM] Notifying {} of term expiration | Term: {}", p_s, new_term);
                        let _ = send_message(p, &msg, cfg_clone2.net_timeout_ms).await;
                    }
                    
                    sleep(StdDuration::from_millis(200)).await;
                }
                
            }
            sleep(StdDuration::from_millis(cfg_clone2.heartbeat_interval_secs)).await;
        }
    });

    loop {
        sleep(StdDuration::from_secs(60)).await;
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    shared: Arc<RwLock<NodeState>>,
    cpu: Arc<RwLock<f32>>,
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
        Message::Heartbeat { leader, term_end_unix, election_term } => {
            let mut ns = shared.write().await;
            
            // Only accept heartbeat from current or newer term
            if election_term >= ns.election_term {
                ns.election_term = election_term;
                ns.last_heartbeat = Some(Instant::now());
                ns.leader = Some(leader.clone());
                ns.election_in_progress = false;

                let now_unix = Utc::now().timestamp() as u64;
                if term_end_unix > now_unix {
                    let remaining = term_end_unix - now_unix;
                    ns.term_end = Some(Instant::now() + StdDuration::from_secs(remaining));
                }
                println!("[HEARTBEAT] Received from {} | Term: {}", leader, election_term);
            } else {
                println!("[HEARTBEAT] Rejected stale heartbeat from {} | Term: {} (current: {})", 
                         leader, election_term, ns.election_term);
            }

            let resp = Message::Ping;
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::GetCpu => {
            let val = { *cpu.read().await };
            let resp = Message::CpuResp { cpu_percent: val, addr: peer.to_string() };
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::LeaderAnnounce { leader, term_end_unix, election_term } => {
            let mut ns = shared.write().await;
            
            // Only accept announcement from current or newer term
            if election_term >= ns.election_term {
                ns.election_term = election_term;
                
                // Check if this is a "stepping down" message (empty leader)
                if leader.is_empty() {
                    // Leader is stepping down, clear state and prepare for election
                    ns.leader = None;
                    ns.state = State::Follower;
                    ns.last_heartbeat = None;  // Force election timeout
                    ns.term_end = None;
                    ns.election_in_progress = false;
                    println!("[ANNOUNCE] Leader stepped down | Term: {} | Clearing leader state", election_term);
                } else {
                    // Normal leader announcement
                    ns.leader = Some(leader.clone());
                    ns.state = State::Follower;
                    ns.last_heartbeat = Some(Instant::now());
                    ns.election_in_progress = false;
                    
                    let now_unix = Utc::now().timestamp() as u64;
                    if term_end_unix > now_unix {
                        let remaining = term_end_unix - now_unix;
                        ns.term_end = Some(Instant::now() + StdDuration::from_secs(remaining));
                    }
                    println!("[ANNOUNCE] New leader: {} | Term: {}", leader, election_term);
                }
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
    cfg: &Config,
    shared: Arc<RwLock<NodeState>>,
    cpu: Arc<RwLock<f32>>,
) -> anyhow::Result<()> {
    // Increment election term
    let new_term = {
        let mut ns = shared.write().await;
        ns.election_term += 1;
        ns.election_term
    };
    
    println!("[ELECTION] Starting election from {} | Term: {}", this_addr_str, new_term);
    let mut collected: HashMap<String, f32> = HashMap::new();
    collected.insert(this_addr_str.to_string(), *cpu.read().await);

    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == this_addr_str {
            continue;
        }
        match request_cpu(p, cfg.net_timeout_ms).await {
            Ok(val) => {
                collected.insert(p_s.clone(), val);
                println!("[ELECTION] Collected CPU {}% from {}", val, p_s);
            }
            Err(e) => {
                eprintln!("[ELECTION] Failed to get CPU from {}: {}", p, e);
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

    if let Some((leader_addr, cpu_val)) = chosen {
        println!("[ELECTION] Election result: {} with {}% CPU | Term: {}", 
                 leader_addr, cpu_val, new_term);
        let term_end_unix =
            (Utc::now() + ChronoDuration::seconds(cfg.leader_term_secs as i64)).timestamp() as u64;

        if leader_addr == this_addr_str {
            {
                let mut ns = shared.write().await;
                ns.state = State::Leader;
                ns.leader = Some(this_addr_str.to_string());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(cfg.leader_term_secs));
                ns.last_heartbeat = Some(Instant::now());
                ns.election_in_progress = false;
            }
            log_state_change(&shared, this_addr_str).await;
            broadcast_leader(&peers, &this_addr_str, term_end_unix, new_term, cfg.net_timeout_ms).await;
        } else {
            {
                let mut ns = shared.write().await;
                ns.state = State::Follower;
                ns.leader = Some(leader_addr.clone());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(cfg.leader_term_secs));
                ns.last_heartbeat = Some(Instant::now());
                ns.election_in_progress = false;
            }
            log_state_change(&shared, this_addr_str).await;
        }
    } else {
        let mut ns = shared.write().await;
        ns.election_in_progress = false;
    }

    Ok(())
}

async fn request_cpu(peer: &SocketAddr, timeout_ms: u64) -> anyhow::Result<f32> {
    let addr = peer.to_string();
    println!("[CPU Request] Connecting to {}", addr);
    let connect =
        tokio::time::timeout(StdDuration::from_millis(timeout_ms), TcpStream::connect(peer)).await;
    let mut stream = match connect {
        Ok(Ok(s)) => {
            println!("[CPU Request] Connected to {}", addr);
            s
        }
        _ => {
            eprintln!("[CPU Request] Failed to connect or timeout to {}", addr);
            anyhow::bail!("connect timeout or failed to {}", addr)
        }
    };

    let msg = Message::GetCpu;
    let s = serde_json::to_string(&msg)? + "\n";
    stream.write_all(s.as_bytes()).await?;
    println!("[CPU Request] Sent GetCpu to {}", addr);

    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    let n = tokio::time::timeout(StdDuration::from_millis(timeout_ms), reader.read_line(&mut buf))
        .await??;

    if n == 0 {
        eprintln!("[CPU Request] No response from {}", addr);
        anyhow::bail!("no response from {}", addr);
    }

    let resp: Message = serde_json::from_str(buf.trim())?;
    if let Message::CpuResp { cpu_percent, .. } = resp {
        println!("[CPU Request] Received CPU {}% from {}", cpu_percent, addr);
        Ok(cpu_percent)
    } else {
        eprintln!("[CPU Request] Unexpected response from {}", addr);
        anyhow::bail!("unexpected response from {}", addr);
    }
}


async fn broadcast_leader(peers: &[SocketAddr], leader: &str, term_end_unix: u64, election_term: u64, timeout_ms: u64) {
    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == leader {
            continue;
        }
        let msg = Message::LeaderAnnounce { 
            leader: leader.to_string(), 
            term_end_unix,
            election_term 
        };
        println!("[BROADCAST] Announcing leadership to {} | Term: {}", p_s, election_term);
        let _ = send_message(p, &msg, timeout_ms).await;
    }
}

async fn send_heartbeat_to_peers(
    peers: &[SocketAddr],
    leader: &str,
    cfg: &Config,
    shared: Arc<RwLock<NodeState>>,
) {
    let (term_end_unix, election_term) = {
        let ns = shared.read().await;
        let term_unix = (Utc::now() + ChronoDuration::seconds(cfg.leader_term_secs as i64)).timestamp() as u64;
        (term_unix, ns.election_term)
    };
    
    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == leader {
            continue;
        }
        let msg = Message::Heartbeat { 
            leader: leader.to_string(), 
            term_end_unix,
            election_term 
        };
        let _ = send_message(p, &msg, cfg.net_timeout_ms).await;
    }
}

async fn send_message(peer: &SocketAddr, msg: &Message, timeout_ms: u64) -> anyhow::Result<()> {
    let addr = peer.to_string();
    println!("[Send] Connecting to {}", addr);
    let connect =
        tokio::time::timeout(StdDuration::from_millis(timeout_ms), TcpStream::connect(peer)).await;

    let mut stream = match connect {
        Ok(Ok(s)) => {
            println!("[Send] Connected to {}", addr);
            s
        }
        _ => {
            eprintln!("[Send] Failed to connect or timeout to {}", addr);
            anyhow::bail!("connect timeout or failed to {}", addr)
        }
    };

    let s = serde_json::to_string(msg)? + "\n";
    stream.write_all(s.as_bytes()).await?;
    println!("[Send] Sent message to {}", addr);

    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    let res = tokio::time::timeout(StdDuration::from_millis(timeout_ms), reader.read_line(&mut buf)).await;

    match res {
        Ok(Ok(0)) => println!("[Send] No response received from {}", addr),
        Ok(Ok(_)) => println!("[Send] Received response from {}", addr),
        _ => eprintln!("[Send] Timeout or error receiving response from {}", addr),
    }

    Ok(())
}

async fn log_state_change(shared: &Arc<RwLock<NodeState>>, this_addr: &str) {
    let ns = shared.read().await;
    let state_str = match ns.state {
        State::Leader => "LEADER",
        State::Follower => "FOLLOWER",
    };
    let leader_str = ns.leader.as_ref().map(|s| s.as_str()).unwrap_or("NONE");
    println!("[STATE] {} | State: {} | Leader: {} | Term: {}", 
             this_addr, state_str, leader_str, ns.election_term);
}