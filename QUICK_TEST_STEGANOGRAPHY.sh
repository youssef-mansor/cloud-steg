#!/bin/bash

echo "=========================================="
echo "Quick Steganography P2P Test"
echo "=========================================="
echo ""

echo "📋 Prerequisites:"
echo "  1. 3 servers running (ports 8000, 8001, 8002)"
echo "  2. Clients registered with images"
echo "  3. Heartbeats running for all clients"
echo "  4. P2P servers running for all clients"
echo ""

read -p "Press Enter when all prerequisites are ready..."

echo ""
echo "Step 1: Discover online users..."
./client/target/release/p2p-client list-online

echo ""
echo "Step 2: Alice requests image from Bob..."
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0

if [ $? -eq 0 ]; then
    echo ""
    echo "Step 3: Alice views the image (view 1/5)..."
    ./client/target/release/p2p-client view-image \
      --username alice \
      --encrypted-image-path data/encrypted_images/alice/bob_image_0.png
    
    echo ""
    echo "Step 4: Check views remaining (view 2/5)..."
    ./client/target/release/p2p-client view-image \
      --username alice \
      --encrypted-image-path data/encrypted_images/alice/bob_image_0.png
    
    echo ""
    echo "✅ Test completed! Check the output above."
else
    echo ""
    echo "❌ Request failed. Make sure:"
    echo "  - Bob's P2P server is running"
    echo "  - Bob is registered with images"
    echo "  - Port 6001 is not blocked"
fi

