# Cloud Steg P2P Discovery System

A distributed peer-to-peer discovery system with leader election and steganographic image sharing, built in Rust. Users can register, discover online peers, and securely share images with view limits using steganography.

## Features

- **Distributed Leader Election**: CPU-based leader election with automatic failover
- **User Registration**: Register with username, IP, port, and sample images
- **Heartbeat System**: Track online users with TTL-based presence
- **Image Discovery**: Share low-resolution previews (128x128px) for discovery
- **P2P Image Sharing**: Direct peer-to-peer image sharing with steganography
- **Access Control**: Encrypted images with username verification and view limits
- **Automatic Leader Discovery**: Clients automatically find and connect to the current leader
- **Multi-Server Support**: Run multiple servers with shared data directory

## Architecture

- **Server**: Actix-web HTTP API server with leader election
- **Client**: CLI client for registration, heartbeats, discovery, and P2P operations
- **P2P Server**: Client-side HTTP server for receiving image requests
- **Leader Election**: CPU-based election with TCP inter-server communication
- **Steganography**: LSB encoding for embedding metadata and images
- **Data Persistence**: JSON-based storage for users, photo requests, and view records

## Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- macOS/Linux (tested on macOS)
- Test images in `test_images/` directory (optional, for testing)

## Building

### Build Server
```bash
cd server
cargo build --release
```

### Build Client
```bash
cd client
cargo build --release
```

The binaries will be in:
- `server/target/release/p2p-discovery-server`
- `client/target/release/p2p-client`

## Configuration

### Server Configuration

The server uses `server/config_3servers.toml` for leader election configuration:

```toml
this_node = "127.0.0.1:5001"
peers = ["127.0.0.1:5002", "127.0.0.1:5003"]
heartbeat_interval_ms = 1000
election_timeout_min_ms = 2000
election_timeout_max_ms = 5000
leader_term_ms = 10000
net_timeout_ms = 1000
cpu_refresh_ms = 500
election_retry_ms = 1000
```

**Important**: For local testing, use `127.0.0.1` for all IPs, not `192.168.x.x` or `10.40.x.x`.

## Complete System Startup Guide

### Step 1: Start 3 Servers (3 Separate Terminals)

All servers must use the **same** `DATA_DIR=data` to share user data.

**Terminal 1 (Server 1):**
```bash
cd server
PORT=8000 DATA_DIR=data ./target/release/p2p-discovery-server \
  --config config_3servers.toml \
  --this-node 127.0.0.1:5001
```

**Terminal 2 (Server 2):**
```bash
cd server
PORT=8001 DATA_DIR=data ./target/release/p2p-discovery-server \
  --config config_3servers.toml \
  --this-node 127.0.0.1:5002
```

**Terminal 3 (Server 3):**
```bash
cd server
PORT=8002 DATA_DIR=data ./target/release/p2p-discovery-server \
  --config config_3servers.toml \
  --this-node 127.0.0.1:5003
```

**Wait 10-15 seconds** for leader election to complete. One server will become the leader.

### Step 2: Register 3 Clients

You can register clients one by one or use the registration script.

**Option A: Manual Registration**

```bash
# Register Alice
./client/target/release/p2p-client register \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --image-paths test_images/test1.jpeg

# Register Bob
./client/target/release/p2p-client register \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --image-paths test_images/test2.jpeg

# Register Charlie
./client/target/release/p2p-client register \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --image-paths test_images/test3.jpeg
```

**Option B: Use Registration Script**

```bash
./REGISTER_3_CLIENTS.sh
```

**Note**: If users already exist, use `./RE_REGISTER_CLIENTS.sh` to clear and re-register.

### Step 3: Start Heartbeats (3 Separate Terminals)

Each client needs to send heartbeats to stay online.

**Terminal 4 (Alice Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --interval 5
```

**Terminal 5 (Bob Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --interval 5
```

**Terminal 6 (Charlie Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --interval 5
```

### Step 4: Start P2P Servers (3 Separate Terminals)

Each client needs a P2P server running to receive image requests.

**Terminal 7 (Alice P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000
```

**Terminal 8 (Bob P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

**Terminal 9 (Charlie P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002
```

## Client Commands Reference

### 1. Register a User

Register a new user with username, IP, port, and images.

```bash
./client/target/release/p2p-client register \
  --username <username> \
  --ip <ip_address> \
  --port <port> \
  --image-paths <image1>,<image2>,...
```

**Example:**
```bash
./client/target/release/p2p-client register \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --image-paths test_images/test1.jpeg,test_images/test2.jpeg
```

### 2. Start Heartbeat

Send periodic heartbeats to keep the user online.

```bash
./client/target/release/p2p-client start-heartbeat \
  --username <username> \
  --ip <ip_address> \
  --port <port> \
  --interval <seconds>
```

**Example:**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --interval 5
```

### 3. Discover Online Users

List all online users with their images.

```bash
./client/target/release/p2p-client list-online
```

**Output Example:**
```
Found leader at: http://localhost:8000

Online users (3):

  👤 alice @ 192.168.1.10:6000
     📷 1 image(s):
        [1] 128x128px (34KB)
        💾 /tmp/p2p_preview_alice_0.png

  👤 bob @ 192.168.1.20:6001
     📷 1 image(s):
        [1] 128x128px (36KB)
        💾 /tmp/p2p_preview_bob_0.png
```

### 4. Request an Image

Request an image from another user (sends request only, doesn't receive image yet).

```bash
./client/target/release/p2p-client request-image \
  --username <your_username> \
  --target-username <target_username> \
  --target-ip <target_ip> \
  --target-port <target_port> \
  --image-index <index>
```

**Example:**
```bash
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0
```

### 5. List Pending Requests

List all pending image requests (for image owners).

```bash
./client/target/release/p2p-client list-requests \
  --username <username> \
  --ip <ip_address> \
  --port <port>
```

**Example:**
```bash
./client/target/release/p2p-client list-requests \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

**Output Example:**
```
📬 Pending requests for bob:
  [0] alice requests image [0] (requested 5s ago)

💡 Use 'send-image' command to approve and send an image
```

### 6. Send/Approve Image

Encrypt and send an image to a requester with chosen view count.

```bash
./client/target/release/p2p-client send-image \
  --username <your_username> \
  --requester-username <requester_username> \
  --image-index <index> \
  --views <view_count> \
  --ip <your_ip> \
  --port <your_port>
```

**Example:**
```bash
./client/target/release/p2p-client send-image \
  --username bob \
  --requester-username alice \
  --image-index 0 \
  --views 5 \
  --ip 192.168.1.20 \
  --port 6001
```

**What happens:**
- Image is encrypted with steganography
- Metadata includes: `allowed_username`, `views_remaining`, `original_username`
- Encrypted image is sent to requester's P2P server
- If delivery fails, image is saved as fallback

### 7. List Received Images

List all encrypted images you've received.

```bash
./client/target/release/p2p-client list-received-images \
  --username <username>
```

**Example:**
```bash
./client/target/release/p2p-client list-received-images \
  --username alice
```

**Output Example:**
```
📬 Encrypted images for alice:
   Directory: data/encrypted_images/alice

  [0] received_image_1764956587.png (518KB)
      Path: data/encrypted_images/alice/received_image_1764956587.png

💡 Use 'view-image' command to decrypt and view an image
```

### 8. View/Decrypt Image

Decrypt and view an encrypted image. Decrements view count and re-encrypts.

```bash
./client/target/release/p2p-client view-image \
  --username <username> \
  --encrypted-image-path <path>
```

**Example:**
```bash
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/received_image_1764956587.png
```

**What happens:**
- Decrypts the steganographic image
- Verifies `allowed_username` matches your username
- Checks `views_remaining > 0`
- Decrements view count (e.g., 5 → 4)
- Saves decrypted image temporarily
- Re-encrypts with updated view count
- Opens image in viewer (macOS)
- When views reach 0, encrypted file is deleted

**Output Example:**
```
Decrypting image: data/encrypted_images/alice/received_image_1764956587.png
✅ Access granted!
   Owner: bob
   Views remaining: 5
✅ Image decrypted and saved to: /tmp/p2p_decrypted_alice_1764956588.png
✅ Image re-encrypted with 4 views remaining
```

### 9. Start P2P Server

Start the client-side P2P server to receive image requests.

```bash
./client/target/release/p2p-client start-p2p-server \
  --username <username> \
  --ip <ip_address> \
  --port <port>
```

**Example:**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000
```

**What it does:**
- Loads original images from `data/original_images/{username}/`
- Listens for incoming image requests
- Handles `/p2p/request-image`, `/p2p/send-image`, `/p2p/list-requests`, `/p2p/receive-image`

## Complete Workflow Example

Here's a complete example of Alice requesting and viewing Bob's image:

### 1. Alice Discovers Users
```bash
./client/target/release/p2p-client list-online
```

### 2. Alice Requests Image from Bob
```bash
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0
```

### 3. Bob Lists Pending Requests
```bash
./client/target/release/p2p-client list-requests \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

### 4. Bob Sends Image to Alice (with 5 views)
```bash
./client/target/release/p2p-client send-image \
  --username bob \
  --requester-username alice \
  --image-index 0 \
  --views 5 \
  --ip 192.168.1.20 \
  --port 6001
```

### 5. Alice Lists Received Images
```bash
./client/target/release/p2p-client list-received-images \
  --username alice
```

### 6. Alice Views the Image (5 times)
```bash
# View 1/5
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/received_image_XXXXX.png

# View 2/5, 3/5, 4/5, 5/5 (repeat the command)
# After 5 views, the file will be deleted
```

## System Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│                    3 Discovery Servers                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                    │
│  │ Server 1 │  │ Server 2 │  │ Server 3 │  (Leader Election)│
│  │  :8000   │  │  :8001   │  │  :8002   │                    │
│  └──────────┘  └──────────┘  └──────────┘                    │
│       │              │              │                          │
│       └──────────────┴──────────────┘                          │
│                    Shared Data Dir                             │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ HTTP API
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
   ┌─────────┐        ┌─────────┐        ┌─────────┐
   │  Alice  │        │   Bob   │        │ Charlie │
   │ :6000   │        │ :6001   │        │ :6002   │
   └─────────┘        └─────────┘        └─────────┘
        │                   │                   │
        │  P2P Server       │  P2P Server       │  P2P Server
        │  (Receives        │  (Receives        │  (Receives
        │   requests)       │   requests)       │   requests)
        │                   │                   │
        └───────────────────┴───────────────────┘
                    Direct P2P Communication
```

## Data Storage

All data is stored in JSON files and directories:

- `server/data/users.json` - Registered users
- `server/data/photo_requests.json` - Photo access requests
- `server/data/view_records.json` - Photo view records
- `data/original_images/{username}/` - Original images for P2P sharing
- `data/encrypted_images/{username}/` - Encrypted images received

## Environment Variables

- `PORT` - Server HTTP port (default: 8000)
- `DATA_DIR` - Data directory for JSON files (default: "data")
- `HEARTBEAT_TTL_SECONDS` - Heartbeat TTL in seconds (default: 10)

## Troubleshooting

### All servers show as leaders
- Check `config_3servers.toml` has correct peer IPs (use `127.0.0.1`, not `10.40.x.x`)
- Ensure all servers use same `DATA_DIR=data`
- Verify TCP ports 5001-5003 are not blocked
- Wait 10-15 seconds after starting for leader election

### Users not showing images
- Ensure all servers use shared `DATA_DIR=data`
- Check that images were registered successfully
- Verify leader has access to shared data directory
- Restart servers if needed

### Client can't find leader
- Ensure at least one server is running
- Check server ports (8000, 8001, 8002)
- Wait 10-15 seconds after starting servers for leader election

### Image request/delivery fails
- Ensure target user's P2P server is running
- Check IP addresses (use `127.0.0.1` for local testing if IPs are `192.168.x.x`)
- Verify ports are correct (6000, 6001, 6002)
- Check firewall settings

### "User already exists" error
- Use `./RE_REGISTER_CLIENTS.sh` to clear and re-register
- Or manually delete `server/data/users.json` and re-register

### P2P server "Can't assign requested address"
- The server binds to `0.0.0.0` automatically, this should not occur
- If it does, check if the port is already in use

## Project Structure

```
cloud-steg-p2p/
├── server/                      # Server code
│   ├── src/main.rs             # Main server implementation
│   ├── config.toml             # Default config
│   ├── config_3servers.toml    # 3-server config
│   └── target/release/         # Built binaries
├── client/                      # Client code
│   ├── src/
│   │   ├── main.rs             # CLI client
│   │   └── steganography.rs    # Steganography logic
│   └── target/release/          # Built binaries
├── test_images/                # Sample test images
├── data/                        # Data directory (shared)
│   ├── original_images/        # Original images by user
│   └── encrypted_images/       # Encrypted images by user
├── server/data/                 # Server data (shared)
│   ├── users.json              # Registered users
│   ├── photo_requests.json     # Photo requests
│   └── view_records.json       # View records
├── REGISTER_3_CLIENTS.sh       # Registration script
├── RE_REGISTER_CLIENTS.sh      # Re-registration script
├── STEGANOGRAPHY_COMMANDS.md   # Detailed command guide
└── README.md                   # This file
```

## Steganography Details

The system uses LSB (Least Significant Bit) steganography to embed:
- **Metadata**: `allowed_username`, `views_remaining`, `original_username`
- **Secret Image**: The actual image to be shared

The encrypted image contains:
- A cover image (800x600 blue-tinted image)
- Embedded metadata in LSB
- Embedded secret image in LSB

When viewing:
- Metadata is extracted and verified
- View count is decremented
- Image is re-encrypted with updated metadata
- When views reach 0, the encrypted file is deleted

## Version

**Current Version**: 2.0.0 (with Steganography P2P)

## License

MIT License
