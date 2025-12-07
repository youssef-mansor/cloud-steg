# Client Testing Instructions

## Configuration
- **Server**: `http://10.40.45.27:3000`
- **Client 1**: Port `8000` (username: `testuser1`)
- **Client 2**: Port `8001` (username: `testuser2`)

## Quick Start

### Option 1: Automated Script
Run the automated script that starts both clients in separate terminals:
```bash
./run-test-clients.sh
```

### Option 2: Manual Commands

#### Step 1: Get your local IP
```bash
LOCAL_IP=$(hostname -I | awk '{print $1}')
echo "Your IP: $LOCAL_IP"
```

#### Step 2: Build the client
```bash
cd client
cargo build --release
```

#### Step 3: Register users (one-time)
Open two terminals and run:

**Terminal 1:**
```bash
cd client
cargo run --release -- register --username testuser1 --addr $(hostname -I | awk '{print $1}'):8000
```

**Terminal 2:**
```bash
cd client
cargo run --release -- register --username testuser2 --addr $(hostname -I | awk '{print $1}'):8001
```

#### Step 4: Start heartbeat clients
Keep the terminals open and run:

**Terminal 1:**
```bash
cargo run --release -- heartbeat --username testuser1 --addr $(hostname -I | awk '{print $1}'):8000 --interval 5
```

**Terminal 2:**
```bash
cargo run --release -- heartbeat --username testuser2 --addr $(hostname -I | awk '{print $1}'):8001 --interval 5
```

## Testing Commands

### Check server status
```bash
cd client
cargo run --release -- status
```

### Discover online users
```bash
cd client
cargo run --release -- discover
```

### List all registered users
```bash
cd client
cargo run --release -- users
```

## Notes
- The client now defaults to connecting to `http://10.40.45.27:3000`
- No need to specify `--servers` flag unless you want to override
- Heartbeat interval is set to 5 seconds
- Clients will auto-reconnect if leader changes
