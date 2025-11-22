# P2P Discovery Server & Client

A Rust-based peer-to-peer discovery system with a server that tracks user presence via heartbeats and a CLI client for registration and discovery.

## Features

- **Server**: Actix-web based discovery server with persistent user storage
- **Client**: CLI tool for registration, heartbeats, and discovery
- **Heartbeat-based presence**: Users are considered online based on TTL (default 3s)
- **No unregister**: Presence is managed entirely through heartbeats

## Project Structure

```
.
├── server/          # Discovery server (actix-web)
├── client/          # CLI client (clap + reqwest)
├── tools/           # Demo scripts
├── data/            # Persistent storage (users.json, client configs)
└── README.md
```

## Prerequisites

- Rust 1.70+ (with cargo)
- Unix-like system (for demo script)

## Building

### Server

```bash
cd server
cargo build --release
```

### Client

```bash
cd client
cargo build --release
```

## Running

### Start the Server

```bash
cd server
cargo run --release
```

Or with custom TTL:

```bash
HEARTBEAT_TTL_SECONDS=60 cargo run --release
```

The server will start on `http://0.0.0.0:8000`.

### Client Commands

#### Register a user

```bash
./client/target/release/p2p-client register \
  --username alice \
  --password secret123 \
  --server http://localhost:8000
```

#### Start sending heartbeats

```bash
./client/target/release/p2p-client start-heartbeat \
  --username alice \
  --server http://localhost:8000 \
  --interval 5
```

This will send a heartbeat every 5 seconds until you press CTRL+C.

#### List online users

```bash
./client/target/release/p2p-client list-online \
  --server http://localhost:8000
```

## Demo Script

Run the automated demo that:
1. Starts the server
2. Registers 3 users (alice, bob, charlie)
3. Starts heartbeats for all 3
4. Queries discovery every 5 seconds
5. Stops one client and shows TTL expiration

```bash
chmod +x tools/run_demo.sh
./tools/run_demo.sh
```

## API Endpoints

### POST /register

Register a new user.

**Request:**
```json
{
  "username": "alice",
  "password": "secret"
}
```

**Response:**
```json
{
  "status": "ok"
}
```

**Error (400):**
```json
{
  "error": "exists"
}
```

### POST /heartbeat

Send a heartbeat to update presence.

**Request:**
```json
{
  "username": "alice"
}
```

**Response:**
```json
{
  "status": "ok",
  "last_seen": "2024-01-15T10:30:00Z"
}
```

### GET /discovery/online

Get list of online users (last_seen within TTL).

**Response:**
```json
{
  "online": ["alice", "bob"]
}
```

### GET /status

Health check endpoint.

**Response:**
```json
{
  "status": "ok"
}
```

## Configuration

- **HEARTBEAT_TTL_SECONDS**: Environment variable to set heartbeat TTL (default: 3 seconds)

## Manual Testing (4 Terminals)

For manual testing with 1 server + 3 clients, see [TESTING_GUIDE.md](TESTING_GUIDE.md) for detailed step-by-step instructions.

Quick setup:

**Terminal 1 - Server:**
```bash
cd server && cargo run --release
```

**Terminal 2 - Client 1 (Alice):**
```bash
./client/target/release/p2p-client register --username alice --password test123 --server http://localhost:8000
./client/target/release/p2p-client start-heartbeat --username alice --server http://localhost:8000 --interval 5
```

**Terminal 3 - Client 2 (Bob):**
```bash
./client/target/release/p2p-client register --username bob --password test123 --server http://localhost:8000
./client/target/release/p2p-client start-heartbeat --username bob --server http://localhost:8000 --interval 5
```

**Terminal 4 - Client 3 (Charlie) + Discovery:**
```bash
./client/target/release/p2p-client register --username charlie --password test123 --server http://localhost:8000
./client/target/release/p2p-client start-heartbeat --username charlie --server http://localhost:8000 --interval 5

# In the same terminal or a 5th terminal, query discovery:
while true; do
  echo "=== $(date) ==="
  ./client/target/release/p2p-client list-online --server http://localhost:8000
  sleep 5
done
```

## Testing

Run server tests:

```bash
cd server
cargo test
```

## Data Storage

- **Server**: Registered users are persisted in `data/users.json`
- **Client**: Client configs are saved in `data/client_<username>.json`
- **Server memory**: `last_seen` timestamps are kept in memory only (reset on server restart)

## Notes

- Users never unregister. Presence is based solely on heartbeats + TTL.
- If a user stops sending heartbeats, they will disappear from the online list after TTL expires.
- Server logs all heartbeats to console.
- CORS is enabled for all origins (for development convenience).

