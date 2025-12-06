# Steganography P2P - Clear Command Guide

## Prerequisites
- 3 servers running (ports 8000, 8001, 8002)
- 3 clients registered (alice, bob, charlie)
- 3 heartbeats running
- 3 P2P servers running

---

## Step-by-Step Commands

### Step 1: Client A (Alice) Discovers Online Users

```bash
./client/target/release/p2p-client list-online
```

**What happens:**
- Alice queries the discovery server
- Shows all online users (alice, bob, charlie)
- Displays their IPs, ports, and image previews

**Expected Output:**
```
Found leader at: http://localhost:8000

Online users (3):

  👤 alice @ 192.168.1.10:6000 [1 image(s), first: ...]
  👤 bob @ 192.168.1.20:6001 [1 image(s), first: ...]
  👤 charlie @ 192.168.1.30:6002 [1 image(s), first: ...]
```

---

### Step 2: Client A (Alice) Requests Image from Client B (Bob)

```bash
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0
```

**What happens:**
- Alice sends request to Bob's P2P server (192.168.1.20:6001)
- Bob's P2P server stores the request (doesn't send image yet)
- Request is pending until Bob approves it

**Expected Output:**
```
📤 Sending image request [0] to bob @ 192.168.1.20:6001
✅ Request sent successfully!
💡 bob needs to approve and send the image using 'send-image' command
```

**On Bob's P2P Server Terminal, you should see:**
```
📥 Image request from 'alice' for image [0]
✅ Request stored. Use 'list-requests' to see pending requests
```

---

### Step 3: Client B (Bob) Lists Pending Requests

```bash
./client/target/release/p2p-client list-requests \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

**What happens:**
- Bob queries his P2P server for pending requests
- Shows all requests waiting for approval

**Expected Output:**
```
📬 Pending requests for bob:
  [0] alice requests image [0] (requested 5s ago)

💡 Use 'send-image' command to approve and send an image
```

---

### Step 4: Client B (Bob) Sends Image to Client A (Alice) with Chosen View Count

```bash
./client/target/release/p2p-client send-image \
  --username bob \
  --requester-username alice \
  --image-index 0 \
  --views 5 \
  --ip 192.168.1.20 \
  --port 6001
```

**What happens:**
- Bob encrypts his image with:
  - `allowed_username: "alice"`
  - `views_remaining: 5` (chosen by Bob)
  - `original_username: "bob"`
- Encrypted image is sent to Alice's P2P server
- Alice receives and saves it to: `data/encrypted_images/alice/received_image_XXXXX.png`

**Expected Output:**
```
📤 Sending image [0] to alice with 5 views
✅ Encrypted image sent to alice @ 192.168.1.10:6000
```

**On Bob's P2P Server Terminal, you should see:**
```
📤 Sending image [0] to 'alice' with 5 views
✅ Encrypted image ready to send to 'alice'
```

**On Alice's P2P Server Terminal, you should see:**
```
✅ Encrypted image received and saved to: data/encrypted_images/alice/received_image_1234567890.png
```

---

### Step 5: Client A (Alice) Views the Encrypted Image

```bash
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/received_image_1234567890.png
```

**What happens:**
- Alice decrypts the encrypted image
- Verifies username matches ("alice")
- Checks views remaining > 0
- Decrements view count (5 → 4)
- Decrypts and saves image temporarily
- Re-encrypts with new view count (4)
- Opens image in viewer (macOS)

**Expected Output:**
```
Decrypting image: data/encrypted_images/alice/received_image_1234567890.png
✅ Access granted!
   Owner: bob
   Views remaining: 5
✅ Image decrypted and saved to: /tmp/p2p_decrypted_alice_XXXXX.png
✅ Image re-encrypted with 4 views remaining
```

---

## Complete Test Sequence

### Terminal 1: Discovery (Client A)
```bash
./client/target/release/p2p-client list-online
```

### Terminal 2: Request Image (Client A)
```bash
./client/target/release/p2p-client request-image \
  --username alice \
  --target-username bob \
  --target-ip 192.168.1.20 \
  --target-port 6001 \
  --image-index 0
```

### Terminal 3: List Requests (Client B)
```bash
./client/target/release/p2p-client list-requests \
  --username bob \
  --ip 192.168.1.20 \
  --port 6001
```

### Terminal 4: Send Image (Client B)
```bash
./client/target/release/p2p-client send-image \
  --username bob \
  --requester-username alice \
  --image-index 0 \
  --views 5 \
  --ip 192.168.1.20 \
  --port 6001
```

### Terminal 5: View Image (Client A - run multiple times to test view count)
```bash
# View 1/5
./client/target/release/p2p-client view-image \
  --username alice \
  --encrypted-image-path data/encrypted_images/alice/received_image_XXXXX.png

# View 2/5, 3/5, 4/5, 5/5 (repeat the command)
# View 6/5 (should fail - no views remaining)
```

---

## Quick Reference

| Action | Command |
|--------|---------|
| **Discover** | `./client/target/release/p2p-client list-online` |
| **Request Image** | `./client/target/release/p2p-client request-image --username alice --target-username bob --target-ip 192.168.1.20 --target-port 6001 --image-index 0` |
| **List Requests** | `./client/target/release/p2p-client list-requests --username bob --ip 192.168.1.20 --port 6001` |
| **Send Image** | `./client/target/release/p2p-client send-image --username bob --requester-username alice --image-index 0 --views 5 --ip 192.168.1.20 --port 6001` |
| **View Image** | `./client/target/release/p2p-client view-image --username alice --encrypted-image-path data/encrypted_images/alice/received_image_XXXXX.png` |

---

## Notes

- **Request and Send are now separate**: Client A requests, Client B approves and sends with chosen view count
- The encrypted image is automatically delivered to Client A's P2P server
- Each view decrements the count and re-encrypts
- When views reach 0, the encrypted file is deleted
- Client B can choose any view count (e.g., 1, 5, 10) when sending the image

