#!/bin/bash

# Script to run two test clients on ports 8000 and 8001
# Server is at 10.40.45.27:3000

# Get the local IP address
LOCAL_IP=$(hostname -I | awk '{print $1}')

echo "========================================="
echo "Starting Test Clients"
echo "========================================="
echo "Server: http://10.40.45.27:3000"
echo "Local IP: $LOCAL_IP"
echo ""
echo "Client 1: ${LOCAL_IP}:8000"
echo "Client 2: ${LOCAL_IP}:8001"
echo "========================================="
echo ""

cd client

# Build the client first
echo "Building client..."
cargo build --release
echo ""

# Start client 1 in the background
echo "Starting Client 1 (testuser1 @ ${LOCAL_IP}:8000)..."
gnome-terminal -- bash -c "cd $(pwd) && cargo run --release -- heartbeat --username testuser1 --addr ${LOCAL_IP}:8000 --interval 5; exec bash" &

sleep 2

# Start client 2 in the background
echo "Starting Client 2 (testuser2 @ ${LOCAL_IP}:8001)..."
gnome-terminal -- bash -c "cd $(pwd) && cargo run --release -- heartbeat --username testuser2 --addr ${LOCAL_IP}:8001 --interval 5; exec bash" &

echo ""
echo "✓ Both clients started in separate terminals"
echo ""
echo "To register users first, run:"
echo "  cd client"
echo "  cargo run --release -- register --username testuser1 --addr ${LOCAL_IP}:8000"
echo "  cargo run --release -- register --username testuser2 --addr ${LOCAL_IP}:8001"
echo ""
echo "To check online users:"
echo "  cargo run --release -- discover"
echo ""
