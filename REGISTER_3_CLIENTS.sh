#!/bin/bash
# Register 3 clients with their images

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLIENT_BIN="$PROJECT_ROOT/client/target/release/p2p-client"

echo "=========================================="
echo "Registering 3 Clients"
echo "=========================================="
echo ""

echo "1️⃣ Registering Alice with test1.jpeg..."
"$CLIENT_BIN" register \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --image-paths test_images/test1.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Alice registered successfully"
else
    echo "❌ Alice registration failed"
    exit 1
fi

echo ""
echo "2️⃣ Registering Bob with test2.jpeg..."
"$CLIENT_BIN" register \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --image-paths test_images/test2.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Bob registered successfully"
else
    echo "❌ Bob registration failed"
    exit 1
fi

echo ""
echo "3️⃣ Registering Charlie with test3.jpeg..."
"$CLIENT_BIN" register \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --image-paths test_images/test3.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Charlie registered successfully"
else
    echo "❌ Charlie registration failed"
    exit 1
fi

echo ""
echo "=========================================="
echo "✅ All 3 clients registered!"
echo "=========================================="
echo ""
echo "Next step: Start heartbeats in 3 terminals"
echo "See START_3_CLIENTS.md for heartbeat commands"

