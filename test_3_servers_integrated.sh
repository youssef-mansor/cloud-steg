#!/bin/bash
# Test script for 3 servers with leader election + photo sharing

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_DIR="$PROJECT_ROOT/server"
SERVER_BIN="$SERVER_DIR/target/release/p2p-discovery-server"

echo "=========================================="
echo "Testing 3 Servers with Leader Election"
echo "=========================================="
echo ""

# Use existing config file
CONFIG_FILE="$SERVER_DIR/config_3servers.toml"
if [ ! -f "$CONFIG_FILE" ]; then
    echo "❌ Config file not found: $CONFIG_FILE"
    echo "Please create it first or use the default config.toml"
    exit 1
fi

echo "✓ Using config: $CONFIG_FILE"
echo ""

# Build server
echo "Building server..."
cd "$SERVER_DIR"
cargo build --release
if [ $? -ne 0 ]; then
    echo "❌ Build failed!"
    exit 1
fi
cd "$PROJECT_ROOT"

echo ""
echo "=========================================="
echo "Starting 3 Servers"
echo "=========================================="
echo ""
echo "Server 1: HTTP on port 8000, Election on 5001"
echo "Server 2: HTTP on port 8001, Election on 5002"
echo "Server 3: HTTP on port 8002, Election on 5003"
echo ""
echo "Press CTRL+C to stop all servers"
echo ""

# Create separate data directories
mkdir -p "$SERVER_DIR/data/server1"
mkdir -p "$SERVER_DIR/data/server2"
mkdir -p "$SERVER_DIR/data/server3"

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "Stopping all servers..."
    kill $SERVER1_PID $SERVER2_PID $SERVER3_PID 2>/dev/null
    wait $SERVER1_PID $SERVER2_PID $SERVER3_PID 2>/dev/null
    echo "All servers stopped."
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start Server 1
echo "Starting Server 1..."
cd "$SERVER_DIR"
DATA_DIR="data/server1" PORT=8000 "$SERVER_BIN" --config "$CONFIG_FILE" --this-node 127.0.0.1:5001 > /tmp/server1.log 2>&1 &
SERVER1_PID=$!
cd "$PROJECT_ROOT"
sleep 2

# Start Server 2
echo "Starting Server 2..."
cd "$SERVER_DIR"
DATA_DIR="data/server2" PORT=8001 "$SERVER_BIN" --config "$CONFIG_FILE" --this-node 127.0.0.1:5002 > /tmp/server2.log 2>&1 &
SERVER2_PID=$!
cd "$PROJECT_ROOT"
sleep 2

# Start Server 3
echo "Starting Server 3..."
cd "$SERVER_DIR"
DATA_DIR="data/server3" PORT=8002 "$SERVER_BIN" --config "$CONFIG_FILE" --this-node 127.0.0.1:5003 > /tmp/server3.log 2>&1 &
SERVER3_PID=$!
cd "$PROJECT_ROOT"
sleep 5

echo ""
echo "Server PIDs:"
echo "  Server 1 (8000/5001): $SERVER1_PID"
echo "  Server 2 (8001/5002): $SERVER2_PID"
echo "  Server 3 (8002/5003): $SERVER3_PID"
echo ""

# Check server status
echo "Checking server status..."
for port in 8000 8001 8002; do
    if curl -s "http://localhost:$port/status" > /dev/null 2>&1; then
        STATUS=$(curl -s "http://localhost:$port/status")
        IS_LEADER=$(echo "$STATUS" | grep -o '"is_leader":[^,]*' | cut -d':' -f2)
        echo "  ✓ Server on port $port is running (is_leader: $IS_LEADER)"
    else
        echo "  ✗ Server on port $port failed to start"
    fi
done

echo ""
echo "=========================================="
echo "Test Commands"
echo "=========================================="
echo ""
echo "Check leader status:"
echo "  curl http://localhost:8000/status | jq"
echo "  curl http://localhost:8001/status | jq"
echo "  curl http://localhost:8002/status | jq"
echo ""
echo "Register user (only leader will accept):"
echo "  curl -X POST http://localhost:8000/register -H 'Content-Type: application/json' -d '{\"username\":\"alice\",\"password\":\"pass\",\"sample_images\":[]}'"
echo ""
echo "View logs:"
echo "  tail -f /tmp/server1.log"
echo "  tail -f /tmp/server2.log"
echo "  tail -f /tmp/server3.log"
echo ""

# Wait for all servers
wait

