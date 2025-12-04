
# Distributed Leader Election + Client Registration & Discovery

A Rust-based distributed system with **leader election** (TCP-based) and **HTTP API** for client registration, heartbeat tracking, and peer discovery.

## HTTP API Endpoints

| Endpoint   | Method | Leader Only | Description                                                  | Request                                | Response                                                              |
|------------|--------|-------------|--------------------------------------------------------------|----------------------------------------|-----------------------------------------------------------------------|
| `/`        | `GET`  | No          | **Health check** + online client count                       | -                                      | `{"status":"ok","is_leader":true,"online_clients_count":2}`           |
| `/register`| `POST` | ✅ Yes      | **Register a new client** (persistent in Google Drive)       | `{"username":"alice","addr":"10.40.44.10:9000"}` | `{"success":true,"message":"User registered","user_id":"uuid"}`       |
| `/heartbeat`| `POST` | ✅ Yes      | **Mark client as online** (in-memory)                        | `{"username":"alice","addr":"10.40.44.10:9000"}` | `{"success":true,"message":"Heartbeat accepted for 'alice' at 10.40.44.10:9000"}` |
| `/users`   | `GET`  | ✅ Yes      | **List ALL registered clients** (persistent from Drive)      | -                                      | `{"users":[{"username":"alice","addr":"10.40.44.10:9000",...}],"count":1}` |
| `/discover`| `GET`  | ✅ Yes      | **List CURRENTLY ONLINE clients** (volatile, in-memory)      | -                                      | `{"online_clients":[{"username":"alice","addr":"10.40.44.10:9000"}],"count":1,"is_leader":true}` |                      |

**Leader-only endpoints** return `403 Forbidden` on followers with current leader info.

## Quick Start (Single PC)

### 1. Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Copy Google Drive credentials (from your rust-drive project)
mkdir -p credentials
cp /path/to/rust-drive/credentials/service-account.json credentials/
````

### 2. Update `config.toml`

```toml
this_node = "127.0.0.1:8080"  # Will be overridden by --this-node
peers = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"]
heartbeat_interval_ms = 1000
election_timeout_min_ms = 3000
election_timeout_max_ms = 5000
leader_term_ms = 10000
net_timeout_ms = 2000
cpu_refresh_ms = 500
election_retry_ms = 100
```

### 3. Run 3 Nodes (Leader Election + HTTP APIs)

**Terminal 1 - Node 1 (Port 8080, API:3000):**

```bash
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA \
REGISTERED_USERS_FOLDER_ID=1P5o3QM-PicdKdYNQM9YRqaNje4fv6QNk \
API_PORT=3000 \
cargo run -- --config config.toml --this-node 127.0.0.1:8080
```

**Terminal 2 - Node 2 (Port 8081, API:3001):**

```bash
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA \
REGISTERED_USERS_FOLDER_ID=1P5o3QM-PicdKdYNQM9YRqaNje4fv6QNk \
API_PORT=3001 \
cargo run -- --config config.toml --this-node 127.0.0.1:8081
```

**Terminal 3 - Node 3 (Port 8082, API:3002):**

```bash
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA \
REGISTERED_USERS_FOLDER_ID=1P5o3QM-PicdKdYNQM9YRqaNje4fv6QNk \
API_PORT=3002 \
cargo run -- --config config.toml --this-node 127.0.0.1:8082
```

**Expected Output:**

```bash
🚀 HTTP API server listening on http://0.0.0.0:3000
   Endpoints:
     GET  /           - Health check
     POST /register   - Register new user
     POST /heartbeat  - Send heartbeat
     GET  /users      - List all registered users
     GET  /discover   - List online clients
✓ Leader election TCP listener bound to 127.0.0.1:8080
```

## Complete Test Flow

### Step 1: Check Health (any node)

```bash
curl http://localhost:3000/
# {"status":"ok","is_leader":true,"online_clients_count":0,"current_leader":null}
```

### Step 2: Register Clients (only on leader)

```bash
curl -X POST http://localhost:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "alice", "addr": "192.168.1.10:9000"}'
# {"success":true,"message":"User 'alice' registered successfully at 192.168.1.10:9000","user_id":"uuid"}

curl -X POST http://localhost:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "bob", "addr": "192.168.1.11:9001"}'
```

### Step 3: Send Heartbeats (simulate online clients)

```bash
# Alice heartbeat loop (every 8s)
while true; do
  curl -X POST http://localhost:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "alice"}'
  sleep 8
done &

# Bob heartbeat loop (every 12s)
while true; do
  curl -X POST http://localhost:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "bob"}'
  sleep 12
done &
```

### Step 4: Test Discovery

```bash
# Online clients only
curl http://localhost:3000/discover
# {"online_clients":["alice","bob"],"count":2,"is_leader":true}

# All registered users (persistent)
curl http://localhost:3000/users
# [{"id":"uuid","username":"alice","addr":"192.168.1.10:9000",...}]
```

### Step 5: Test Leader Election

```bash
# Kill leader node (Ctrl+C on Terminal 1)
# Watch other terminals: new leader elected within ~5 seconds

# Try endpoints on old leader port (should fail)
curl http://localhost:3000/discover
# Connection refused

# Try on new leader (port 3001)
curl http://localhost:3001/discover
# {"online_clients":[],"count":0,"is_leader":true}
# (online list empty - new leader starts fresh)
```

### Step 6: Test Heartbeat Timeout

```bash
# Stop one heartbeat loop (Ctrl+C)
# Wait 35 seconds...

# Check discovery
curl http://localhost:3001/discover
# {"online_clients":["bob"],"count":1}  (alice timed out)
```

### Step 7: Test Follower Rejection

```bash
# Try heartbeat on follower
curl -X POST http://localhost:3002/heartbeat \
  -H "Content-Type: application/json" \
  -d '{"username": "charlie"}'
# {"success":false,"message":"This node is not the leader. Current leader: 127.0.0.1:8081"}
```

## Client Usage Pattern

```bash
# 1. Register once
curl -X POST http://LEADER_IP:3000/register \
  -d '{"username":"myclient","addr":"MY_IP:9000"}'

# 2. Send heartbeat every 10s
while true; do
  curl -X POST http://LEADER_IP:3000/heartbeat \
    -d '{"username":"myclient"}'
  sleep 10
done &

# 3. Discover peers
curl http://LEADER_IP:3000/discover
# Connect to discovered peers at their addr:port
```

## Environment Variables

| Variable                     | Required | Default                            | Description                           |
| ---------------------------- | -------- | ---------------------------------- | ------------------------------------- |
| `SHARED_DRIVE_ID`            | ✅ Yes    | -                                  | Google Shared Drive ID                |
| `REGISTERED_USERS_FOLDER_ID` | ✅ Yes    | -                                  | Existing "registered-users" folder ID |
| `API_PORT`                   | No       | `3000`                             | HTTP API port                         |
| `GOOGLE_CREDENTIALS`         | No       | `credentials/service-account.json` | Service account JSON                  |
| `RUST_LOG`                   | No       | `info`                             | Logging level                         |

## Architecture

```
Clients ── heartbeat(10s) ──→ Leader ── online list (30s timeout) ──→ /discover
              │                        │
              └── register ───────────→ Google Drive (persistent)
                                         ↑
                                      /users
```

* **Leader election:** TCP, CPU-based
