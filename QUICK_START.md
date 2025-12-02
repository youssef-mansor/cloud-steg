# Quick Start Guide - View Count System

## Server Setup

```bash
cd server
cargo build --release
./target/release/p2p-discovery-server
```

Server will listen on `http://0.0.0.0:8000`

## Quick Test Commands

### 1. Register users with images
```bash
curl -X POST http://localhost:8000/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "secret123",
    "sample_images": ["base64_image_1", "base64_image_2"]
  }'
```

### 2. Send heartbeat (to go online)
```bash
curl -X POST http://localhost:8000/heartbeat \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "ip": "192.168.1.10",
    "port": 6000
  }'
```

### 3. Request photo access
```bash
curl -X POST http://localhost:8000/photo/request/bob \
  -H "Content-Type: application/json" \
  -d '{
    "owner": "alice",
    "photo_id": "0",
    "message": "Can I see your photo?"
  }'
```

### 4. Check pending requests (owner)
```bash
curl http://localhost:8000/photo/requests/alice
```

### 5. Approve request with view limit
```bash
curl -X POST http://localhost:8000/photo/approve/alice \
  -H "Content-Type: application/json" \
  -d '{
    "request_id": "REQUEST_ID_FROM_STEP_3",
    "approved": true,
    "max_views": 5,
    "expiry_hours": 24
  }'
```

### 6. Check access records (requester)
```bash
curl http://localhost:8000/photo/access/bob
```

### 7. View original photo
```bash
curl -X POST http://localhost:8000/photo/view/bob \
  -H "Content-Type: application/json" \
  -d '{
    "request_id": "REQUEST_ID_FROM_STEP_3"
  }'
```

Response includes:
- `success`: true/false
- `image_data`: base64 original image (if approved)
- `views_remaining`: count of remaining views
- `message`: error message if any

### 8. Deny request
```bash
curl -X POST http://localhost:8000/photo/approve/alice \
  -H "Content-Type: application/json" \
  -d '{
    "request_id": "REQUEST_ID",
    "approved": false,
    "max_views": 0
  }'
```

## Data Files

Created in `data/` directory:
- `users.json` - User accounts with sample images
- `photo_requests.json` - Pending photo requests
- `view_records.json` - Approved access with view tracking

## Testing

Run the comprehensive test:
```bash
./test_view_count.sh
```

This will:
1. Register 3 users (alice, bob, charlie)
2. Test photo request workflow
3. Test approval with view limits
4. Verify view counting (3 views, 4th fails)
5. Test request denial
6. Verify discovery integration

All operations should complete successfully! ✅
