# Manual Testing Guide - 4 Terminals

This guide shows you how to test the P2P discovery system using 4 terminals: 1 server + 3 clients.

## Prerequisites

First, build both the server and client:

```bash
# Build server
cd server
cargo build --release
cd ..

# Build client
cd client
cargo build --release
cd ..
```

## Terminal Setup

### Terminal 1: Server

```bash
cd server
cargo run --release
```

You should see:
```
Starting P2P Discovery Server on http://0.0.0.0:8000
Heartbeat TTL: 3 seconds
```

**Keep this terminal running!**

---

### Terminal 2: Client 1 (Alice)

```bash
# Register Alice
./client/target/release/p2p-client register \
  --username alice \
  --password test123 \
  --server http://localhost:8000

# Start heartbeat (runs until CTRL+C)
./client/target/release/p2p-client start-heartbeat \
  --username alice \
  --server http://localhost:8000 \
  --interval 5
```

You should see heartbeat confirmations every 5 seconds:
```
[alice] Heartbeat OK - last_seen: 2024-01-15T10:30:00Z
```

**Keep this terminal running!**

---

### Terminal 3: Client 2 (Bob)

```bash
# Register Bob
./client/target/release/p2p-client register \
  --username bob \
  --password test123 \
  --server http://localhost:8000

# Start heartbeat (runs until CTRL+C)
./client/target/release/p2p-client start-heartbeat \
  --username bob \
  --server http://localhost:8000 \
  --interval 5
```

**Keep this terminal running!**

---

### Terminal 4: Client 3 (Charlie) + Discovery Queries

```bash
# Register Charlie
./client/target/release/p2p-client register \
  --username charlie \
  --password test123 \
  --server http://localhost:8000

# Start heartbeat in background (or use a separate terminal)
./client/target/release/p2p-client start-heartbeat \
  --username charlie \
  --server http://localhost:8000 \
  --interval 5 &

# Now query discovery periodically
while true; do
  echo "=== $(date) ==="
  ./client/target/release/p2p-client list-online --server http://localhost:8000
  echo ""
  sleep 5
done
```

Or, if you want to run Charlie's heartbeat separately, use **Terminal 5** for discovery queries:

```bash
# Query discovery every 5 seconds
while true; do
  echo "=== $(date) ==="
  ./client/target/release/p2p-client list-online --server http://localhost:8000
  echo ""
  sleep 5
done
```

---

## Testing Scenarios

### Test 1: All 3 clients online
- Start all 3 heartbeats
- Query discovery - should see all 3 users

### Test 2: Stop one client (simulate offline)
- Stop one client's heartbeat (CTRL+C in that terminal)
- Wait 3+ seconds (TTL expiration)
- Query discovery - that user should disappear

### Test 3: Restart the stopped client
- Start the heartbeat again for the stopped user
- Query discovery - user should reappear

### Test 4: Check server logs
- Look at Terminal 1 (server) - you should see heartbeat logs:
```
Heartbeat from: alice at 2024-01-15T10:30:00Z
Heartbeat from: bob at 2024-01-15T10:30:01Z
Heartbeat from: charlie at 2024-01-15T10:30:02Z
```

---

## Quick Commands Reference

```bash
# Register
./client/target/release/p2p-client register --username <name> --password <pass> --server http://localhost:8000

# Start heartbeat
./client/target/release/p2p-client start-heartbeat --username <name> --server http://localhost:8000 --interval 5

# List online users
./client/target/release/p2p-client list-online --server http://localhost:8000
```

---

## Cleanup

To stop everything:

1. Stop all client heartbeats (CTRL+C in each client terminal)
2. Stop the server (CTRL+C in server terminal)

Or kill all processes:
```bash
pkill -f "p2p-client"
pkill -f "p2p-discovery-server"
```

