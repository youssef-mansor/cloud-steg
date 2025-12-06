#!/bin/bash

echo "=== Testing Steganography P2P ==="
echo ""

echo "1. Discovering online users..."
./client/target/release/p2p-client list-online

echo ""
echo "2. Alice requesting image from Bob..."
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0

echo ""
echo "3. Alice viewing the image (view 1/5)..."
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png

echo ""
echo "4. Alice viewing again (view 2/5)..."
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png

echo ""
echo "✅ Test complete!"
