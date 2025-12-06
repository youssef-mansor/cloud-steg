
# Distributed Leader Election + Client Registration & Discovery

A Rust-based distributed system with **leader election** (TCP-based) and **HTTP API** for client registration, heartbeat tracking, peer discovery, image storage, and bulk image retrieval.

## Features

- ✅ **Leader Election**: CPU-based, TCP protocol with randomized timeouts
- ✅ **Client Registration**: Persistent storage in Firebase Storage
- ✅ **Heartbeat Tracking**: In-memory online status (30s timeout)
- ✅ **Peer Discovery**: Query currently online clients
- ✅ **Image Upload**: Store images per user (max 128×128)
- ✅ **Bulk Image Retrieval**: Download all online clients' images in one request
- ✅ **Leader-only Operations**: Followers redirect to current leader

***

## HTTP API Endpoints

| Endpoint   | Method | Leader Only | Description                                                  | Request                                | Response                                                              |
|------------|--------|-------------|--------------------------------------------------------------|----------------------------------------|-----------------------------------------------------------------------|
| `/`        | `GET`  | No          | **Health check** + online client count                       | -                                      | `{"status":"ok","is_leader":true,"online_clients_count":2}`           |
| `/register`| `POST` | ✅ Yes      | **Register a new client** (persistent in Firebase)           | `{"username":"alice","addr":"10.40.6.26:9000"}` | `{"success":true,"message":"User registered","user_id":"uuid"}`       |
| `/heartbeat`| `POST` | ✅ Yes      | **Mark client as online** (in-memory, 30s timeout)          | `{"username":"alice","addr":"10.40.6.26:9000"}` | `{"success":true,"message":"Heartbeat accepted for 'alice' at 10.40.6.26:9000"}` |
| `/users`   | `GET`  | ✅ Yes      | **List ALL registered clients** (persistent from Firebase)   | -                                      | `{"users":[{"username":"alice","addr":"10.40.6.26:9000",...}],"count":1}` |
| `/discover`| `GET`  | ✅ Yes      | **List CURRENTLY ONLINE clients** (volatile, in-memory)      | -                                      | `{"online_clients":[{"username":"alice","addr":"10.40.6.26:9000"}],"count":1,"is_leader":true}` |
| `/discover_with_images` | `GET` | ✅ Yes | **List online clients WITH images** (base64, max 20 per user) | - | `{"online_clients":[{"username":"alice","addr":"...","images":[{"filename":"...","data":"base64..."}]}],"count":1}` |
| `/upload_image/:username` | `POST` | ✅ Yes | **Upload image for user** (max 128×128, registered users only) | Multipart form data: `image` field | `{"success":true,"message":"Image uploaded","filename":"timestamp-uuid.png"}` |
| `/images/:username` | `GET` | ✅ Yes | **List all images for a user** | - | `{"images":["1733511234-a1b2.png","1733512000-c3d4.jpg"],"count":2}` |
| `/image/:username/:filename` | `GET` | ✅ Yes | **Download specific image** | - | Binary image data |

**Leader-only endpoints** return `403 Forbidden` on followers with current leader info.

***

## Firebase Storage Structure

```
bucket-root/
  users/
    alice/
      profile.json              # Registration data
      images/
        1733511234-a1b2c3d4.png
        1733512000-e5f6g7h8.jpg
    bob/
      profile.json
      images/
        1733513000-xyz123.png
```

**Benefits:**
- ✅ Fast user lookup: `users/{username}/profile.json`
- ✅ Easy image retrieval: `users/{username}/images/*`
- ✅ Organized per-user data
- ✅ No scanning required

***

## Quick Start (Single PC)

### 1. Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Setup Firebase credentials
mkdir -p credentials
# Place your firebase-adminsdk JSON file in credentials/
```

### 2. Configure Environment Variables

**Option A: Use the provided script (recommended)**

```bash
# Edit define-variables.sh with your values
nano define-variables.sh

# Source it before running
source define-variables.sh
```

**Option B: Export manually**

```bash
export FIREBASE_BUCKET="cloud-steg-a463f.firebasestorage.app"
export GOOGLE_APPLICATION_CREDENTIALS="credentials/cloud-steg-a463f-firebase-adminsdk-fbsvc-484ddc44b2.json"
export API_PORT=3000
```

### 3. Update `config.toml`

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

### 4. Run 3 Nodes (Leader Election + HTTP APIs)

**Terminal 1 - Node 1 (Port 8080, API:3000):**

```bash
source define-variables.sh
cargo run -- --config config.toml --this-node 127.0.0.1:8080
```

**Terminal 2 - Node 2 (Port 8081, API:3001):**

```bash
source define-variables.sh
export API_PORT=3001
cargo run -- --config config.toml --this-node 127.0.0.1:8081
```

**Terminal 3 - Node 3 (Port 8082, API:3002):**

```bash
source define-variables.sh
export API_PORT=3002
cargo run -- --config config.toml --this-node 127.0.0.1:8082
```

**Expected Output:**

```
===========================================
Distributed System: Leader Election + User Registration
===========================================

✓ User registration system initialized (Firebase Storage)
🚀 HTTP API server listening on http://0.0.0.0:3000
   Endpoints:
     GET  /                        - Health check
     POST /register                - Register new user
     POST /heartbeat               - Send heartbeat
     GET  /users                   - List all registered users
     GET  /discover                - List online clients
     GET  /discover_with_images    - List online clients with images
     POST /upload_image/:username  - Upload image (max 128x128)
     GET  /images/:username        - List user's images
     GET  /image/:username/:file   - Download specific image

✓ Leader election TCP listener bound to 127.0.0.1:8080
✓ All systems operational!
```

***

## Complete Test Flow

### Step 1: Check Health (any node)

```bash
curl http://localhost:3000/
# {"status":"ok","is_leader":true,"online_clients_count":0,"current_leader":"127.0.0.1:8080"}
```

### Step 2: Register Clients (only on leader)

```bash
curl -X POST http://localhost:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "alice", "addr": "10.40.6.26:9000"}'
# {"success":true,"message":"User 'alice' registered successfully at 10.40.6.26:9000","user_id":"uuid"}

curl -X POST http://localhost:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "bob", "addr": "10.40.6.26:9001"}'
```

### Step 3: Upload Images (registered users only)

```bash
# Create a test image (128x128)
convert -size 128x128 xc:blue test_image.png

# Upload for alice
curl -X POST http://localhost:3000/upload_image/alice \
  -F "image=@test_image.png"
# {"success":true,"message":"Image uploaded successfully","filename":"1733511234-a1b2c3d4.png"}

# Upload more images
convert -size 100x100 xc:red test_red.png
curl -X POST http://localhost:3000/upload_image/alice \
  -F "image=@test_red.png"

curl -X POST http://localhost:3000/upload_image/bob \
  -F "image=@test_image.png"

# List alice's images
curl http://localhost:3000/images/alice
# {"images":["1733511234-a1b2c3d4.png","1733511235-e5f6g7h8.png"],"count":2}

# Download specific image
curl http://localhost:3000/image/alice/1733511234-a1b2c3d4.png --output downloaded.png
```

### Step 4: Send Heartbeats (simulate online clients)

```bash
# Alice heartbeat loop (every 10s)
while true; do
  curl -s -X POST http://localhost:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "alice", "addr": "10.40.6.26:9000"}' > /dev/null
  sleep 10
done &

# Bob heartbeat loop (every 10s)
while true; do
  curl -s -X POST http://localhost:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "bob", "addr": "10.40.6.26:9001"}' > /dev/null
  sleep 10
done &
```

### Step 5: Test Discovery

```bash
# Online clients only (metadata)
curl http://localhost:3000/discover
# {"online_clients":[{"username":"alice","addr":"10.40.6.26:9000"},{"username":"bob","addr":"10.40.6.26:9001"}],"count":2,"is_leader":true}

# Online clients WITH images (base64 encoded)
curl http://localhost:3000/discover_with_images
# {"online_clients":[{"username":"alice","addr":"10.40.6.26:9000","images":[{"filename":"1733511234-a1b2c3d4.png","data":"iVBORw0KGgo..."}]}],"count":2}

# All registered users (persistent)
curl http://localhost:3000/users
# {"users":[{"id":"uuid","username":"alice","addr":"10.40.6.26:9000",...}],"count":2}
```

### Step 6: Bulk Download with Script

```bash
# Make script executable
chmod +x download_online_clients.sh

# Download all online clients' images
./download_online_clients.sh http://localhost:3000

# Output:
# ===================================
# Downloading Online Clients & Images
# ===================================
# Leader: http://localhost:3000
# Output: ./online_clients
#
# Fetching data from http://localhost:3000/discover_with_images...
#
# Client: alice @ 10.40.6.26:9000
#   Images: 2
#     ✓ 1733511234-a1b2c3d4.png
#     ✓ 1733511235-e5f6g7h8.png
#
# Client: bob @ 10.40.6.26:9001
#   Images: 1
#     ✓ 1733513000-xyz123.png
#
# ===================================
# Download complete!
# Files saved to: ./online_clients/
# ===================================
```

**Result directory structure:**

```
./online_clients/
  alice/
    info.json                     # {"username":"alice","addr":"10.40.6.26:9000"}
    1733511234-a1b2c3d4.png
    1733511235-e5f6g7h8.png
  bob/
    info.json
    1733513000-xyz123.png
```

### Step 7: Test Leader Election

```bash
# Kill leader node (Ctrl+C on Terminal 1)
# Watch other terminals: new leader elected within ~5 seconds

# Health check shows new leader
curl http://localhost:3001/
# {"status":"ok","is_leader":true,"online_clients_count":0,"current_leader":"127.0.0.1:8081"}
```

### Step 8: Test Heartbeat Timeout

```bash
# Stop one heartbeat loop (kill background job)
jobs  # Find job number
kill %1  # Kill alice's heartbeat loop

# Wait 35 seconds...

# Check discovery
curl http://localhost:3000/discover
# {"online_clients":[{"username":"bob","addr":"10.40.6.26:9001"}],"count":1}  (alice timed out)
```

### Step 9: Test Follower Rejection

```bash
# Try heartbeat on follower
curl -X POST http://localhost:3002/heartbeat \
  -H "Content-Type: application/json" \
  -d '{"username": "charlie", "addr": "10.40.6.26:9002"}'
# {"success":false,"message":"This node is not the leader. Current leader: 127.0.0.1:8081"}
```

***

## Bulk Image Download Script

### Usage

```bash
./download_online_clients.sh [LEADER_URL]

# Examples:
./download_online_clients.sh http://10.40.6.26:3000
./download_online_clients.sh http://localhost:3000
```

### Requirements

- `curl` - HTTP client
- `jq` - JSON processor
- `base64` - Base64 decoder (usually pre-installed)

Install `jq` if needed:

```bash
# Ubuntu/Debian
sudo apt-get install jq

# macOS
brew install jq
```

### Features

- ✅ Downloads all online clients' images in single request
- ✅ Creates organized directory structure
- ✅ Max 20 images per user (configurable in endpoint)
- ✅ Saves user metadata (`info.json`)
- ✅ Shows progress and summary
- ✅ Handles users with no images

***

## Cross-Network Usage

### Running on Multiple Physical Machines

**On each machine:**

```bash
# 1. Get your local IP
ip addr show | grep "inet " | grep -v 127.0.0.1
# Example: 10.40.6.26

# 2. Update define-variables.sh
export FIREBASE_BUCKET="cloud-steg-a463f.firebasestorage.app"
export GOOGLE_APPLICATION_CREDENTIALS="credentials/cloud-steg-a463f-firebase-adminsdk-fbsvc-484ddc44b2.json"
export API_PORT=3000

# 3. Source and run
source define-variables.sh
cargo run -- --config config.toml --this-node 10.40.6.26:8080
```

**From another device on the same network:**

```bash
# Register
curl -X POST http://10.40.6.26:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "remote_client", "addr": "10.40.6.50:9000"}'

# Discover peers
curl http://10.40.6.26:3000/discover

# Bulk download images
./download_online_clients.sh http://10.40.6.26:3000
```

***

## Client Usage Pattern

```bash
# 1. Register once (persistent)
curl -X POST http://LEADER_IP:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username":"myclient","addr":"MY_IP:9000"}'

# 2. Send heartbeat every 10s (keeps you online)
while true; do
  curl -s -X POST http://LEADER_IP:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username":"myclient","addr":"MY_IP:9000"}' > /dev/null
  sleep 10
done &

# 3. Upload images (optional)
curl -X POST http://LEADER_IP:3000/upload_image/myclient \
  -F "image=@my_image.png"

# 4. Discover online peers
curl http://LEADER_IP:3000/discover
# Use returned addresses to connect peer-to-peer

# 5. Bulk download all peers' images
./download_online_clients.sh http://LEADER_IP:3000
```

***

## Environment Variables

| Variable                     | Required | Default                            | Description                           |
| ---------------------------- | -------- | ---------------------------------- | ------------------------------------- |
| `FIREBASE_BUCKET`            | ✅ Yes    | -                                  | Firebase Storage bucket name          |
| `GOOGLE_APPLICATION_CREDENTIALS` | ✅ Yes | `credentials/firebase-storage.json` | Service account JSON path     |
| `API_PORT`                   | No       | `3000`                             | HTTP API port                         |
| `RUST_LOG`                   | No       | `info`                             | Logging level (debug, info, warn)     |

**Setting up `define-variables.sh`:**

```bash
#!/bin/bash
export FIREBASE_BUCKET="cloud-steg-a463f.firebasestorage.app"
export GOOGLE_APPLICATION_CREDENTIALS="credentials/cloud-steg-a463f-firebase-adminsdk-fbsvc-484ddc44b2.json"
export API_PORT=3000
```

**Usage:**
```bash
source define-variables.sh
cargo run -- --config config.toml --this-node <IP>:<PORT>
```

***

## Architecture

```
Clients ── heartbeat(10s) ──→ Leader ── online list (30s timeout) ──→ /discover
              │                        │                              ──→ /discover_with_images
              ├── register ───────────→ Firebase Storage (persistent)
              │                         ├─ users/{username}/profile.json
              └── upload_image ────────→ └─ users/{username}/images/*.png
                                         ↑
                                      /users, /images, bulk download
```

**Key Components:**
- **Leader election:** TCP, CPU-based with random timeouts (3-5s)
- **Registration:** Firebase Storage (`users/{username}/profile.json`)
- **Images:** Firebase Storage (`users/{username}/images/*`)
- **Online tracking:** In-memory HashMap (resets on leader change)
- **Heartbeat:** 10s interval, 30s timeout
- **Bulk retrieval:** JSON + base64, max 20 images per user

***

## Image Upload Rules

1. ✅ **Max dimensions:** 128×128 pixels
2. ✅ **Formats:** PNG, JPEG, WebP
3. ✅ **Authentication:** User must be registered first
4. ✅ **Naming:** `{timestamp}-{uuid}.{ext}` (unique, sortable)
5. ✅ **Leader-only:** Upload only works on current leader
6. ✅ **Bulk limit:** `/discover_with_images` returns max 20 images per user

**Example upload validation error:**
```bash
curl -X POST http://localhost:3000/upload_image/alice \
  -F "image=@large_image.png"
# {"success":false,"message":"Upload failed: Validation error: Image too large: 256x256 (max 128x128)"}
```

***

## Troubleshooting

**"Failed to initialize user registration":**
- Check `FIREBASE_BUCKET` is correct (format: `project-id.firebasestorage.app`)
- Verify credentials file path in `GOOGLE_APPLICATION_CREDENTIALS`
- Ensure service account has "Storage Admin" role

**"403 Forbidden" on API calls:**
- You're hitting a follower node
- Check leader with: `curl http://NODE_IP:PORT/`
- Use the `current_leader` address from the response

**Heartbeat timeout issues:**
- Ensure client sends heartbeat every 10s
- Check network connectivity
- Verify you're sending to the current leader

**Image upload fails:**
- Verify image is ≤128×128: `identify image.png`
- Check user is registered: `curl http://LEADER/users`
- Ensure format is PNG/JPEG/WebP

**Bulk download script fails:**
- Install `jq`: `sudo apt-get install jq`
- Check you're connected to leader node
- Ensure at least one client is online and has images

***

## Development

**Run tests:**
```bash
cargo test
```

**Test Firebase connectivity:**
```bash
source define-variables.sh
cargo run --bin test_firebase
```

**Enable debug logging:**
```bash
export RUST_LOG=debug
source define-variables.sh
cargo run -- --config config.toml --this-node 127.0.0.1:8080
```

***

## License

MIT