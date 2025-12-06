#!/bin/bash

echo "=========================================="
echo "Re-registering 3 Clients (clearing old data)"
echo "=========================================="
echo ""

# Find leader server
LEADER=$(curl -s http://localhost:8000/status 2>/dev/null | jq -r '.is_leader // false')
if [ "$LEADER" = "true" ]; then
    LEADER_URL="http://localhost:8000"
else
    LEADER=$(curl -s http://localhost:8001/status 2>/dev/null | jq -r '.is_leader // false')
    if [ "$LEADER" = "true" ]; then
        LEADER_URL="http://localhost:8001"
    else
        LEADER_URL="http://localhost:8002"
    fi
fi

echo "Using leader: $LEADER_URL"
echo ""

# Delete existing users from server database
echo "Clearing existing user data..."
if [ -f "server/data/users.json" ]; then
    # Remove alice, bob, charlie from users.json
    cat server/data/users.json | jq 'map(select(.username != "alice" and .username != "bob" and .username != "charlie"))' > /tmp/users_clean.json
    mv /tmp/users_clean.json server/data/users.json
    echo "✅ Cleared existing users from server"
fi

# Remove local client configs
rm -f data/client_alice.json data/client_bob.json data/client_charlie.json
echo "✅ Cleared local client configs"

# Remove old original images
rm -rf data/original_images/alice data/original_images/bob data/original_images/charlie
echo "✅ Cleared old original images"

echo ""
echo "Now registering fresh clients..."
echo ""

# Register Alice
echo "1️⃣ Registering Alice..."
./client/target/release/p2p-client register \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --image-paths test_images/test1.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Alice registered"
else
    echo "❌ Alice registration failed"
    exit 1
fi

# Register Bob
echo ""
echo "2️⃣ Registering Bob..."
./client/target/release/p2p-client register \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --image-paths test_images/test2.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Bob registered"
else
    echo "❌ Bob registration failed"
    exit 1
fi

# Register Charlie
echo ""
echo "3️⃣ Registering Charlie..."
./client/target/release/p2p-client register \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --image-paths test_images/test3.jpeg

if [ $? -eq 0 ]; then
    echo "✅ Charlie registered"
else
    echo "❌ Charlie registration failed"
    exit 1
fi

echo ""
echo "=========================================="
echo "✅ All 3 clients re-registered!"
echo "=========================================="
echo ""
echo "Original images saved to:"
echo "  - data/original_images/alice/image_0.png"
echo "  - data/original_images/bob/image_0.png"
echo "  - data/original_images/charlie/image_0.png"
