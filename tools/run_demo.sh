#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "=========================================="
echo "P2P Discovery Server Demo"
echo "=========================================="
echo ""

# Clean up any existing data
echo "Cleaning up old data..."
rm -f data/users.json
rm -f data/client_*.json

# Build server and client
echo "Building server..."
cd server
cargo build --release
cd ..

echo "Building client..."
cd client
cargo build --release
cd ..

echo ""
echo "Starting server on port 8000..."
cd server
cargo run --release &
SERVER_PID=$!
cd ..

# Wait for server to start
echo "Waiting for server to start..."
sleep 3

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "ERROR: Server failed to start"
    exit 1
fi

echo "Server started (PID: $SERVER_PID)"
echo ""

# Register users
echo "Registering users..."
"$PROJECT_ROOT/client/target/release/p2p-client" register --username alice --password test123 --server http://localhost:8000
"$PROJECT_ROOT/client/target/release/p2p-client" register --username bob --password test123 --server http://localhost:8000
"$PROJECT_ROOT/client/target/release/p2p-client" register --username charlie --password test123 --server http://localhost:8000

echo ""
echo "Starting client heartbeats..."

# Start client heartbeats in background
"$PROJECT_ROOT/client/target/release/p2p-client" start-heartbeat --username alice --server http://localhost:8000 --interval 5 > /tmp/p2p_alice.log 2>&1 &
ALICE_PID=$!

"$PROJECT_ROOT/client/target/release/p2p-client" start-heartbeat --username bob --server http://localhost:8000 --interval 5 > /tmp/p2p_bob.log 2>&1 &
BOB_PID=$!

"$PROJECT_ROOT/client/target/release/p2p-client" start-heartbeat --username charlie --server http://localhost:8000 --interval 5 > /tmp/p2p_charlie.log 2>&1 &
CHARLIE_PID=$!

echo "Alice heartbeat started (PID: $ALICE_PID)"
echo "Bob heartbeat started (PID: $BOB_PID)"
echo "Charlie heartbeat started (PID: $CHARLIE_PID)"
echo ""

# Function to query online users
query_online() {
    echo "--- Querying online users ---"
    "$PROJECT_ROOT/client/target/release/p2p-client" list-online --server http://localhost:8000
    echo ""
}

# Query every 5 seconds for ~30 seconds (6 queries)
echo "Querying discovery endpoint every 5 seconds..."
for i in {1..6}; do
    query_online
    if [ $i -lt 6 ]; then
        sleep 5
    fi
done

echo "Stopping Charlie's heartbeat (simulating offline)..."
kill $CHARLIE_PID 2>/dev/null || true
wait $CHARLIE_PID 2>/dev/null || true

echo ""
echo "Charlie stopped. Waiting for TTL to expire (30 seconds)..."
echo "Querying every 5 seconds to observe Charlie disappearing..."

# Query every 5 seconds for ~35 seconds to see Charlie disappear
for i in {1..7}; do
    query_online
    sleep 5
done

echo ""
echo "=========================================="
echo "Demo complete!"
echo "=========================================="
echo ""
echo "Server is still running (PID: $SERVER_PID)"
echo "Alice heartbeat is still running (PID: $ALICE_PID)"
echo "Bob heartbeat is still running (PID: $BOB_PID)"
echo ""
echo "You can manually test:"
echo "  $PROJECT_ROOT/client/target/release/p2p-client list-online --server http://localhost:8000"
echo ""
echo "To stop everything, run:"
echo "  kill $SERVER_PID $ALICE_PID $BOB_PID"
echo ""
echo "Or press CTRL+C to stop this script (server and clients will continue running)"

# Keep script running
wait

