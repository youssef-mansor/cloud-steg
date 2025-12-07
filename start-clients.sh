#!/bin/bash

# Simple script to register and start two test clients
# Server: http://10.40.45.27:3000
# Client 1: testuser1 @ 10.7.17.14:8000
# Client 2: testuser2 @ 10.7.17.14:8001

cd /home/barbary/cloud-steg/client

echo "========================================="
echo "Starting Test Clients"
echo "========================================="
echo "Server: http://10.40.45.27:3000"
echo "Client 1: testuser1 @ 10.7.17.14:8000"
echo "Client 2: testuser2 @ 10.7.17.14:8001"
echo "========================================="
echo ""

# Start client 1 in a new terminal
echo "Starting Client 1..."
gnome-terminal -- bash -c 'cd /home/barbary/cloud-steg/client && echo "Registering testuser1..." && cargo run -- register --username testuser1 --addr 10.7.17.14:8000 && echo "" && echo "Starting heartbeat for testuser1..." && cargo run -- heartbeat --username testuser1 --addr 10.7.17.14:8000 --interval 5; exec bash' &

sleep 2

# Start client 2 in a new terminal  
echo "Starting Client 2..."
gnome-terminal -- bash -c 'cd /home/barbary/cloud-steg/client && echo "Registering testuser2..." && cargo run -- register --username testuser2 --addr 10.7.17.14:8001 && echo "" && echo "Starting heartbeat for testuser2..." && cargo run -- heartbeat --username testuser2 --addr 10.7.17.14:8001 --interval 5; exec bash' &

echo ""
echo "✓ Both clients started in separate terminals"
echo ""
echo "To check status, run:"
echo "  cd /home/barbary/cloud-steg/client"
echo "  cargo run -- discover"
echo ""
