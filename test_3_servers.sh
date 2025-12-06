#!/bin/bash
# Test script for 3 servers with leader election

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_DIR="$PROJECT_ROOT/server"

echo "=========================================="
echo "Testing 3 Servers with Leader Election"
echo "=========================================="
echo ""

# Create config for 3 local servers
cat > "$SERVER_DIR/config_3servers.toml" <<EOF
this_node = "127.0.0.1:5001"
peers = [
  "127.0.0.1:5001",
  "127.0.0.1:5002",
  "127.0.0.1:5003",
]
heartbeat_interval_ms = 300
election_timeout_min_ms = 9000
election_timeout_max_ms = 15000
leader_term_ms = 30000
net_timeout_ms = 1000
cpu_refresh_ms = 500
election_retry_ms = 200
EOF

echo "Created config_3servers.toml"
echo ""
echo "To test, run in 3 separate terminals:"
echo ""
echo "Terminal 1 (Server 1):"
echo "  cd $SERVER_DIR"
echo "  PORT=8000 cargo run --release -- --config config_3servers.toml --this-node 127.0.0.1:5001"
echo ""
echo "Terminal 2 (Server 2):"
echo "  cd $SERVER_DIR"
echo "  PORT=8001 cargo run --release -- --config config_3servers.toml --this-node 127.0.0.1:5002"
echo ""
echo "Terminal 3 (Server 3):"
echo "  cd $SERVER_DIR"
echo "  PORT=8002 cargo run --release -- --config config_3servers.toml --this-node 127.0.0.1:5003"
echo ""
echo "Then test leader election:"
echo "  curl http://localhost:8000/status"
echo "  curl http://localhost:8001/status"
echo "  curl http://localhost:8002/status"
echo ""
echo "One should show is_leader: true, others should show is_leader: false"

