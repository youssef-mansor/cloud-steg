# Testing Steganography P2P Features

## Prerequisites

- 3 servers running (ports 8000, 8001, 8002)
- 3 clients registered (alice, bob, charlie)

## Complete Testing Workflow

### Step 1: Start 3 Servers (3 Terminals)

**Terminal 1:**
```bash
cd server
PORT=8000 DATA_DIR=data ./target/release/p2p-discovery-server --config config_3servers.toml --this-node 127.0.0.1:5001
```

**Terminal 2:**
```bash
cd server
PORT=8001 DATA_DIR=data ./target/release/p2p-discovery-server --config config_3servers.toml --this-node 127.0.0.1:5002
```

**Terminal 3:**
```bash
cd server
PORT=8002 DATA_DIR=data ./target/release/p2p-discovery-server --config config_3servers.toml --this-node 127.0.0.1:5003
```

Wait 10-15 seconds for leader election.

### Step 2: Register 3 Clients (if not already registered)

```bash
# Register Alice
./client/target/release/p2p-client register \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --image-paths test_images/test1.jpeg

# Register Bob
./client/target/release/p2p-client register \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --image-paths test_images/test2.jpeg

# Register Charlie
./client/target/release/p2p-client register \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --image-paths test_images/test3.jpeg
```

Or use the script:
```bash
./REGISTER_3_CLIENTS.sh
```

### Step 3: Start Heartbeats (3 Terminals)

**Terminal 4 (Alice Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000 \
  --interval 5
```

**Terminal 5 (Bob Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001 \
  --interval 5
```

**Terminal 6 (Charlie Heartbeat):**
```bash
./client/target/release/p2p-client start-heartbeat \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002 \
  --interval 5
```

### Step 4: Start P2P Servers (3 Terminals)

**Terminal 7 (Alice P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username alice \
  --ip 192.168.1.10 \
  --port 6000
```

**Terminal 8 (Bob P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

**Terminal 9 (Charlie P2P Server):**
```bash
./client/target/release/p2p-client start-p2p-server \
  --username charlie \
  --ip 192.168.1.30 \
  --port 6002
```

**Note:** Each client needs BOTH heartbeat AND P2P server running. You can run them in the same terminal by running heartbeat in background, or use separate terminals.

### Step 5: Discover Online Users

**Terminal 10:**
```bash
./client/target/release/p2p-client list-online
```

You should see all 3 users with their images.

### Step 6: Test Image Request (Alice requests from Bob)

**Terminal 11:**
```bash
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0
```

**Expected Output:**
- Alice sends request to Bob's P2P server
- Bob encrypts image with alice's username and 5 views
- Encrypted image saved to: `data/encrypted_images/alice/bob_image_0.png`

### Step 7: Test View Image (Alice views Bob's image)

**Terminal 11 (same terminal):**
```bash
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png
```

**Expected Output:**
- Image decrypted
- Username verified (alice)
- View count: 5 → 4
- Image re-encrypted with 4 views remaining
- Decrypted image opened in image viewer

### Step 8: Test Multiple Views

Run the view-image command 4 more times (views: 4→3→2→1→0):

```bash
# View 2
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png

# View 3
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png

# View 4
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png

# View 5 (last view)
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/bob_image_0.png
```

**Expected:**
- Views decrement: 4 → 3 → 2 → 1 → 0
- After 5th view, encrypted file is deleted
- 6th view attempt shows error: "No views remaining"

### Step 9: Test Access Control (Charlie tries to view Alice's encrypted image)

**Terminal 12:**
```bash
# First, Alice requests image from Charlie
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username charlie \
  --target-ip 192.168.1.30 \
  --target-port 6002 \
  --image-index 0

# Then, Charlie tries to view it (should fail - wrong username)
./client/target/release/p2p-client view-image \
  --username charlie \
  --encrypted-image-path data/encrypted_images/alice/charlie_image_0.png
```

**Expected:** Error: "Access denied: This image is encrypted for 'alice', not 'charlie'"

## Quick Test Script

Save this as `test_steganography.sh`:

```bash
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
```

## Terminal Summary

You'll need **9 terminals** total:

1. Server 1 (port 8000)
2. Server 2 (port 8001)
3. Server 3 (port 8002)
4. Alice Heartbeat
5. Bob Heartbeat
6. Charlie Heartbeat
7. Alice P2P Server
8. Bob P2P Server
9. Charlie P2P Server

**Optional terminals for testing:**
10. Discovery/Testing commands
11. Image requests/views

## Troubleshooting

### "No original images found"
- Make sure you registered with `--image-paths`
- Check `data/original_images/{username}/` exists

### "Connection refused" when requesting image
- Ensure target client's P2P server is running
- Check IP and port are correct
- Verify target client is online (heartbeat running)

### "Access denied" when viewing
- Verify username matches the encrypted image's allowed_username
- Check view count hasn't reached 0

### Image not decrypting
- Ensure you're using the correct encrypted image path
- Check the image file wasn't corrupted

