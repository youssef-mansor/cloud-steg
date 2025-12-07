# cloud-steg

**Run Command**
cargo run -- --config config.toml --this-node <ip>:<port>

peers = [
 "10.40.45.27:5000",
 "10.40.36.216:5000",
 "10.40.54.163:5000",
]


**NEW**
# Terminal 1 - Node 1 (also runs HTTP API on port 3000)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA REGISTERED_USERS_FOLDER_ID=1P5o3QM-PicdKdYNQM9YRqaNje4fv6QNk API_PORT=3000 cargo run -- --config config.toml --this-node 10.40.45.27:5000

# Terminal 2 - Node 2 (HTTP API on port 3001)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3001 cargo run -- --config config.toml --this-node 10.40.36.216:5000

# Terminal 3 - Node 3 (HTTP API on port 3002)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3002 cargo run -- --config config.toml --this-node 10.40.54.163:5000


**Register Service**
curl -X POST http://10.40.6.26:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "youssef", "addr": "192.168.1.50:6666"}'


**Heartbeat Test**
curl http://10.40.45.27:3000/

# Cloud Steganography - Distributed Image Sharing System

A distributed peer-to-peer image sharing system with steganography-based security. Images are encrypted with AES-256 and hidden in cover images using LSB steganography, with automatic view count management.

---

## Quick Start Guide

### Prerequisites

1. **Rust** (for cluster servers)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Node.js 18+** (for UI)
   ```bash
   # Ubuntu/Debian
   curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
   sudo apt-get install -y nodejs
   ```

3. **Firebase Storage** (optional - for remote thumbnails)
   - Create a Firebase project
   - Download service account credentials
   - Place in `credentials/` directory

---

### Step 1: Clone the Repository

```bash
git clone https://github.com/youssef-mansor/cloud-steg.git
cd cloud-steg
```

---

### Step 2: Start Cluster Servers

**Option A: Single Machine (Development)**

```bash
# Build the server
cargo build --release

# Start 3 server instances
./target/release/cloud-steg &
./target/release/cloud-steg &
./target/release/cloud-steg &
```

**Option B: Distributed Cluster (Production)**

On each server machine:
```bash
# Build
cargo build --release

# Configure cluster peers in config.toml
# Edit peers list with your server IPs

# Start one instance per machine
./target/release/cloud-steg
```

Servers will run on port **3000** (HTTP API) and **5000** (inter-cluster).

---

### Step 3: Install and Start UI

```bash
cd user-ui

# Install dependencies
npm install

# Configure server endpoints (optional)
# Edit server.js line 12-15 to match your cluster IPs
# Default: ['http://10.40.45.27:3000', 'http://10.10.36.216:3000', 'http://10.40.54.163:3000']

# Start UI instances (one per user for testing)
PORT=8000 CLIENT_IP=<your-ip> node server.js &
PORT=8001 CLIENT_IP=<your-ip> node server.js &

# Or use PM2 for production
npm install -g pm2
pm2 start server.js --name ui-8000 -- --port 8000
pm2 start server.js --name ui-8001 -- --port 8001
```

**Environment Variables:**
- `PORT`: UI server port (default: 5000)
- `CLIENT_IP`: Your machine's IP address for cluster registration
- `SERVER_ENDPOINTS`: Comma-separated list of cluster servers (optional)

---

### Step 4: Access the UI

Open in your browser:
- User 1: `http://localhost:8000`
- User 2: `http://localhost:8001`

---

## Using the System

### 1. Register and Login

1. Open the UI at `http://localhost:8000`
2. Click **Register** and enter a username (e.g., "alice")
3. Click **Login** with the same username

### 2. Upload Images

1. Go to **My Images** tab
2. Click **Upload Image (128×128)**
3. Select an image file
4. Image is saved locally (original + thumbnail) and thumbnail uploaded to cluster

### 3. Browse Other Users

1. Go to **Browse Users** tab
2. Click **Refresh** to see online users
3. Click **▼ Expand All** to see all users' images
4. Click on any image to request access

### 4. Approve Requests with Steganography

1. Go to **Requests** tab
2. See pending requests from other users
3. Click **Approve** on a request
4. Upload a **cover image** (any size, ideally 512×512+)
5. Set **view count** (how many times they can view)
6. Click **Approve with Cover Image**

**What happens:**
- Original image is encrypted with AES-256
- Encrypted data is hidden in the cover image using LSB steganography
- Steg image is saved to requester's viewable folder

### 5. View Encrypted Images

1. Go to **Viewable** tab (as the requester)
2. See approved images with view counts
3. Click **VIEW** on any image
4. The original image is decrypted and displayed
5. View count decrements automatically
6. Image auto-deletes when count reaches zero

---

## Architecture

### Distributed Cluster
- **Leader Election**: Raft-based consensus (automatic failover)
- **Data Replication**: User registry replicated across all nodes
- **Broadcast Model**: UI floods requests to all servers, uses leader's response

### Steganography
- **Encryption**: AES-256 with derived keys (`SHA256(sender+recipient+imageName)`)
- **Embedding**: LSB (Least Significant Bit) steganography in PNG images
- **Security**: No plaintext storage, deterministic key derivation, view-once capability

### Storage
- **Local**: Original images, thumbnails, requests, viewable steg images
- **Remote**: 128×128 thumbnails on Firebase Storage (optional)
- **Structure**:
  ```
  user-ui/data/
    alice/
      images/
        123-original-photo.jpg   # Full quality
        123-thumb-photo.png      # 128×128 local
      requests/
        456-bob.json             # Pending request
      viewable/
        steg-789.png             # Encrypted image
        steg-789.png.json        # Metadata
  ```

---

## API Endpoints

### Cluster Server (Port 3000)
- `POST /register` - Register user
- `POST /heartbeat` - Keep-alive
- `GET /users` - List registered users
- `GET /discover` - List online users
- `POST /upload_image/:username` - Upload thumbnail
- `GET /images/:username` - List user's images
- `GET /image/:username/:filename` - Serve image

### UI Server (Port 8000+)
- `POST /api/register` - Register user (broadcasts to cluster)
- `POST /api/login` - Login (broadcasts to cluster)
- `POST /api/upload` - Upload image (saves local + remote)
- `GET /api/my-images` - List own images
- `GET /api/user-images/:username` - List user's images
- `GET /api/image/:username/:filename` - Serve image
- `GET /api/discover` - Browse online users
- `POST /api/request-view` - Request image access
- `GET /api/requests` - List pending requests
- `POST /api/approve` - Approve with steganography
- `POST /api/reject` - Reject request
- `GET /api/viewable` - List approved steg images
- `POST /api/view-image` - Decrypt and view image

---

## Configuration

### Server Cluster (`config.toml`)
```toml
[peers]
servers = [
  "10.40.45.27:5000",
  "10.40.36.216:5000",
  "10.40.54.163:5000"
]

[timeouts]
heartbeat_secs = 30
election_timeout_min_ms = 3000
election_timeout_max_ms = 6000
```

### UI Server (`user-ui/server.js`)
```javascript
const serverEndpoints = [
  'http://10.40.45.27:3000',
  'http://10.10.36.216:3000',
  'http://10.40.54.163:3000'
];
```