#!/bin/bash
# Script to run 3 server instances on different ports
# Note: Currently these are independent servers without leader election

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_BIN="$PROJECT_ROOT/server/target/release/p2p-discovery-server"

echo "=========================================="
echo "Starting 3 Server Instances"
echo "=========================================="
echo ""
echo "Server 1: http://localhost:8000"
echo "Server 2: http://localhost:8001"
echo "Server 3: http://localhost:8002"
echo ""
echo "Note: These are currently independent servers."
echo "      Leader election is NOT yet implemented."
echo ""
echo "Press CTRL+C to stop all servers"
echo "=========================================="
echo ""

# Create separate data directories for each server
mkdir -p "$PROJECT_ROOT/data/server1"
mkdir -p "$PROJECT_ROOT/data/server2"
mkdir -p "$PROJECT_ROOT/data/server3"

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

# Start Server 1 on port 8000
echo "Starting Server 1 on port 8000..."
cd "$PROJECT_ROOT/server"
DATA_DIR="$PROJECT_ROOT/data/server1" PORT=8000 "$SERVER_BIN" > /tmp/server1.log 2>&1 &
SERVER1_PID=$!
cd "$PROJECT_ROOT"

# Start Server 2 on port 8001
echo "Starting Server 2 on port 8001..."
cd "$PROJECT_ROOT/server"
DATA_DIR="$PROJECT_ROOT/data/server2" PORT=8001 "$SERVER_BIN" > /tmp/server2.log 2>&1 &
SERVER2_PID=$!
cd "$PROJECT_ROOT"

# Start Server 3 on port 8002
echo "Starting Server 3 on port 8002..."
cd "$PROJECT_ROOT/server"
DATA_DIR="$PROJECT_ROOT/data/server3" PORT=8002 "$SERVER_BIN" > /tmp/server3.log 2>&1 &
SERVER3_PID=$!
cd "$PROJECT_ROOT"

echo ""
echo "Server PIDs:"
echo "  Server 1 (8000): $SERVER1_PID"
echo "  Server 2 (8001): $SERVER2_PID"
echo "  Server 3 (8002): $SERVER3_PID"
echo ""

# Wait a bit for servers to start
sleep 3

# Check if servers are running
echo "Checking server status..."
for port in 8000 8001 8002; do
    if curl -s "http://localhost:$port/status" > /dev/null 2>&1; then
        echo "  ✓ Server on port $port is running"
    else
        echo "  ✗ Server on port $port failed to start"
    fi
done

echo ""
echo "Servers are running. Check logs:"
echo "  tail -f /tmp/server1.log"
echo "  tail -f /tmp/server2.log"
echo "  tail -f /tmp/server3.log"
echo ""
echo "Test commands:"
echo "  curl http://localhost:8000/status"
echo "  curl http://localhost:8001/status"
echo "  curl http://localhost:8002/status"
echo ""

# Wait for all servers
wait

