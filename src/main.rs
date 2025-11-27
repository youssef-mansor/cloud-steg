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
use rand::Rng;

fn random_election_timeout(cfg: &Config) -> u64 {
    rand::thread_rng().gen_range(cfg.election_timeout_min_ms..=cfg.election_timeout_max_ms)
}


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
    startup_time: Instant,
    current_term: u64,
    cpu_snapshot: f32,
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
        startup_time: Instant::now(),
        current_term: 0,
        cpu_snapshot: 0.0,
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
        let mut election_timeout = random_election_timeout(&cfg_clone);  // Generate initial random timeout
        
        loop {
            {
                let ns = shared_clone.read().await;
                if ns.state == State::Follower {
                    let should_elect = if let Some(last) = ns.last_heartbeat {
                        println!("Last heartbeat received, elapsed: {} ms, current term: {}, timeout: {} ms", 
                                last.elapsed().as_millis(), ns.current_term, election_timeout);
                        last.elapsed().as_millis() as u64 >= election_timeout
                    } else {
                        println!("No heartbeat received yet, elapsed: {} ms, current term: {}, timeout: {} ms", 
                                ns.startup_time.elapsed().as_millis(), ns.current_term, election_timeout);
                        // Wait 2x timeout before first election attempt
                        ns.startup_time.elapsed().as_millis() as u64 >= (election_timeout)
                    };
                    
                    if should_elect {
                        drop(ns);
                        if let Err(e) =
                            run_election(&peers_clone, &this_addr_str, &cfg_clone, shared_clone.clone(), cpu.clone()).await
                        {
                            eprintln!("election failed: {}", e);
                        }
                        // Generate NEW random timeout after election attempt
                        election_timeout = random_election_timeout(&cfg_clone);
                        println!("New random election timeout: {} ms", election_timeout);
                    }
                } else if ns.state == State::Leader {
                    // Reset timeout when we're leader
                    election_timeout = random_election_timeout(&cfg_clone);
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
        Message::Heartbeat { leader, term_end_unix, term } => {
            let mut ns = shared.write().await;
            
            // Only accept heartbeats from current or higher term
            if term >= ns.current_term {
                // If higher term, update and step down if leader
                if term > ns.current_term {
                    ns.current_term = term;
                    if ns.state == State::Leader {
                        println!("Stepping down: received heartbeat from higher term {}", term);
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
                println!("Rejected heartbeat from term {} (current term: {})", term, ns.current_term);
            }

            let resp = Message::Ping;
            let s = serde_json::to_string(&resp)? + "\n";
            w.write_all(s.as_bytes()).await?;
        }
        Message::GetCpu { term, initiator_addr, initiator_cpu } => {
            let snapshot_val = {
                let mut ns = shared.write().await;
                
                // If this is a new term, update our snapshot
                if term > ns.current_term {
                    ns.current_term = term;
                    ns.cpu_snapshot = *cpu.read().await;  // Take snapshot for this term
                }
                
                ns.cpu_snapshot  // Return the snapshot for this term
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
                        println!(
                            "[LEADER_ANNOUNCE] Stepping down: received leader announce for term {} while leader in term {}",
                            term, ns.current_term
                        );
                        ns.state = State::Follower;
                    }
                }

                let is_self = match std::env::var("THIS_NODE_ADDR") {
                    Ok(this_addr) => this_addr == leader,
                    Err(_) => false,
                };

                if is_self {
                    println!(
                        "[LEADER_ANNOUNCE] I ({}) am elected leader for term {}",
                        leader, term
                    );
                } else {
                    println!(
                        "[LEADER_ANNOUNCE] New leader {} for term {} (I become follower)",
                        leader, term
                    );
                }

                ns.leader = Some(leader);
                let now_unix = Utc::now().timestamp() as u64;
                if term_end_unix > now_unix {
                    let remaining = term_end_unix - now_unix;
                    ns.term_end = Some(Instant::now() + StdDuration::from_secs(remaining));
                } else {
                    ns.term_end = None;
                }
                ns.state = State::Follower;
                ns.last_heartbeat = Some(Instant::now());
            } else {
                println!(
                    "[LEADER_ANNOUNCE] Rejected leader announce from term {} (current term: {})",
                    term, ns.current_term
                );
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
    // Increment term at start of election
        // Increment term and snapshot CPU atomically
    let (election_term, self_cpu_snapshot) = {
        let mut ns = shared.write().await;
        ns.current_term += 1;
        ns.cpu_snapshot = *cpu.read().await;  // Snapshot for this term
        (ns.current_term, ns.cpu_snapshot)
    };
    
    println!("Starting election from {} for term {} with CPU snapshot: {}%", 
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
                eprintln!("failed to get cpu from {}: {}", p, e);
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
        println!("Election result: leader -> {} (term {})", leader_addr, election_term);
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
            println!(
                "[ELECTION] I ({}) won term {}. Broadcasting LeaderAnnounce to peers",
                this_addr_str, election_term
            );
            broadcast_leader(&peers, &this_addr_str, term_end_unix, election_term, cfg.net_timeout_ms).await;
        } else {
            {
                let mut ns = shared.write().await;
                ns.state = State::Follower;
                ns.leader = Some(leader_addr.clone());
                ns.term_end = Some(Instant::now() + StdDuration::from_millis(cfg.leader_term_ms));
                ns.last_heartbeat = Some(Instant::now());
            }
            println!(
                "[ELECTION] {} won term {} (I am {}). Broadcasting LeaderAnnounce",
                leader_addr, election_term, this_addr_str
            );
            broadcast_leader(&peers, &leader_addr, term_end_unix, election_term, cfg.net_timeout_ms).await;
        }

    }


    Ok(())
}


async fn request_cpu(peer: &SocketAddr, timeout_ms: u64, term: u64, initiator_addr: &str, initiator_cpu: f32) -> anyhow::Result<f32> {
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

    let msg = Message::GetCpu {  
        term, 
        initiator_addr: initiator_addr.to_string(),
        initiator_cpu 
    };
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
    if let Message::CpuResp { cpu_percent, term, .. } = resp {
        println!("[CPU Request] Received CPU {}% from {} (term: {})", cpu_percent, addr, term);
        Ok(cpu_percent)
    }
    else {
        eprintln!("[CPU Request] Unexpected response from {}", addr);
        anyhow::bail!("unexpected response from {}", addr);
    }
}


async fn broadcast_leader(peers: &[SocketAddr], leader: &str, term_end_unix: u64, term: u64, timeout_ms: u64) {
    for p in peers.iter() {
        let p_s = p.to_string();
        if p_s == leader {
            continue;
        }
        println!(
            "[BROADCAST] Announcing leader {} for term {} to {}",
            leader, term, p_s
        );
        let leader_s = leader.to_string();
        let msg = Message::LeaderAnnounce { leader: leader_s.clone(), term_end_unix, term };
        let _ = send_message(p, &msg, timeout_ms).await;
    }
}



async fn send_heartbeat_to_peers(
    peers: &[SocketAddr],
    leader: &str,
    cfg: &Config,
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
