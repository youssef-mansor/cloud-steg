# Cloud Steganography - P2P Image Sharing with View Limits

A distributed peer-to-peer image sharing system with steganography-based view count enforcement. This project combines a distributed server cluster with leader election and P2P client communication for secure, controlled image sharing.

## Project Structure

```
cloud-steg/
├── Cargo.toml              # Workspace configuration
├── server/                 # Distributed server with leader election
│   ├── Cargo.toml
│   ├── config.toml         # Server configuration
│   └── src/
│       ├── main.rs         # Server entry point
│       ├── api.rs          # HTTP API endpoints
│       └── registration/   # User registration module
├── client/                 # P2P client with steganography
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # Client CLI
│       └── steganography.rs # Image encoding/decoding
└── data/                   # Client data directory
    ├── original_images/    # Sample images per user
    └── encrypted_images/   # Received encrypted images
```

## Features

### Server
- **Leader Election**: Distributed consensus using CPU-based election
- **User Registration**: Register users with sample images
- **Heartbeat Tracking**: Track online/offline status of users
- **Discovery Service**: List all currently online users

### Client
- **User Registration**: Register with the server (sample images)
- **Heartbeat**: Periodic heartbeat to stay online
- **P2P Image Requests**: Request images from other users
- **View-Limited Images**: Steganography embeds view count metadata
- **P2P Server**: Receive and respond to image requests

## Prerequisites

- **Rust** (1.70 or later): https://rustup.rs/
- **Google Cloud Service Account** (for server): credentials for Google Drive storage
- **Environment Variables** (for server):
  - `GOOGLE_CREDENTIALS`: Path to service account JSON
  - `SHARED_DRIVE_ID`: Google Drive shared folder ID
  - `REGISTERED_USERS_FOLDER_ID`: Folder ID for user data
  - `API_PORT`: HTTP API port (default: 3000)

## Building

Build both server and client from the workspace root:

```bash
cd cloud-steg
cargo build --release
```

Or build individually:

```bash
# Server only
cd server
cargo build --release

# Client only
cd client
cargo build --release
```

## Running the Server

### Configuration

Edit `server/config.toml` to configure your cluster:

```toml
# This node's address
this_node = "127.0.0.1:5000"

# All cluster peers (including this node)
peers = [
  "127.0.0.1:5000",
  "127.0.0.1:5001",
  "127.0.0.1:5002",
]

# Timing configuration
heartbeat_interval_ms = 300
election_timeout_min_ms = 9000
election_timeout_max_ms = 15000
leader_term_ms = 30000
net_timeout_ms = 1000
cpu_refresh_ms = 500
election_retry_ms = 200
```

### Starting a Server Node

```bash
cd server

# Set required environment variables
export GOOGLE_CREDENTIALS="path/to/credentials.json"
export SHARED_DRIVE_ID="your_shared_drive_id"
export REGISTERED_USERS_FOLDER_ID="your_folder_id"
export API_PORT=3000

# Run with default config
cargo run --release

# Or specify config and node address
cargo run --release -- --config config.toml --this-node "127.0.0.1:5000"
```

### Server API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Health check, returns leader status |
| `/register` | POST | Register a new user |
| `/heartbeat` | POST | Send heartbeat to stay online |
| `/discover` | GET | List all online users |

**Note**: Only the leader node processes requests. Non-leader nodes return 403 Forbidden.

## Running the Client

### Commands Overview

```bash
cd client
cargo run --release -- <COMMAND>
```

Available commands:

| Command | Description |
|---------|-------------|
| `register` | Register a new user with sample images |
| `start-heartbeat` | Start periodic heartbeats to stay online |
| `list-online` | List all online users |
| `start-p2p-server` | Start P2P server to handle image requests |
| `request-image` | Request an image from another user |
| `send-image` | Approve and send an image with view limit |
| `list-requests` | List pending image requests |
| `view-image` | View an encrypted image (decrements view count) |
| `list-received-images` | List received encrypted images |

### Example Workflow

#### 1. Register Users

```bash
# Register Alice
cargo run --release -- register \
  --username alice \
  --ip 127.0.0.1 \
  --port 9001 \
  --image-paths "data/original_images/alice/image_0.png" \
  --server http://localhost:3000

# Register Bob
cargo run --release -- register \
  --username bob \
  --ip 127.0.0.1 \
  --port 9002 \
  --image-paths "data/original_images/bob/image_0.png" \
  --server http://localhost:3000
```

#### 2. Start Heartbeats (in separate terminals)

```bash
# Alice's heartbeat
cargo run --release -- start-heartbeat \
  --username alice \
  --ip 127.0.0.1 \
  --port 9001 \
  --server http://localhost:3000 \
  --interval 5

# Bob's heartbeat
cargo run --release -- start-heartbeat \
  --username bob \
  --ip 127.0.0.1 \
  --port 9002 \
  --server http://localhost:3000 \
  --interval 5
```

#### 3. Start P2P Servers (in separate terminals)

```bash
# Alice's P2P server
cargo run --release -- start-p2p-server \
  --username alice \
  --ip 127.0.0.1 \
  --port 9001

# Bob's P2P server
cargo run --release -- start-p2p-server \
  --username bob \
  --ip 127.0.0.1 \
  --port 9002
```

#### 4. List Online Users

```bash
cargo run --release -- list-online --server http://localhost:3000
```

#### 5. Request an Image

```bash
# Bob requests Alice's image
cargo run --release -- request-image \
  --username bob \
  --target-username alice \
  --target-ip 127.0.0.1 \
  --target-port 9001 \
  --image-index 0 \
  --server http://localhost:3000
```

#### 6. Approve and Send Image (with view limit)

```bash
# Alice checks pending requests
cargo run --release -- list-requests \
  --username alice \
  --ip 127.0.0.1 \
  --port 9001

# Alice approves with 3 views allowed
cargo run --release -- send-image \
  --username alice \
  --requester-username bob \
  --image-index 0 \
  --views 3 \
  --ip 127.0.0.1 \
  --port 9001
```

#### 7. View Encrypted Image

```bash
# Bob lists received images
cargo run --release -- list-received-images --username bob

# Bob views the image (decrements view count)
cargo run --release -- view-image \
  --username bob \
  --encrypted-image-path "data/encrypted_images/bob/from_alice_image_0.png"
```

## Steganography Format

Images are embedded with metadata using LSB steganography:

```json
{
  "owner": "alice",
  "recipient": "bob",
  "image_index": 0,
  "max_views": 3,
  "views_remaining": 3,
  "created_at": "2024-01-01T00:00:00Z",
  "nonce": 12345
}
```

The view count is decremented each time the image is viewed. When `views_remaining` reaches 0, the image can no longer be viewed.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Server 1   │────│  Server 2   │────│  Server 3   │
│  (Leader)   │     │  (Follower) │     │  (Follower) │
└──────┬──────┘     └─────────────┘     └─────────────┘
       │
       │ HTTP API (register, heartbeat, discover)
       │
┌──────┴──────┐
│   Clients   │
├─────────────┤
│   Alice     │───────P2P───────│   Bob       │
│  (P2P:9001) │                 │  (P2P:9002) │
└─────────────┘                 └─────────────┘
```

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test --package p2p-client
cargo test --package dist_leader
```

### Logging

Set `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run --release
```

## License

MIT License
