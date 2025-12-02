# View Count Mechanism API Documentation

## Overview

The view count mechanism allows users to share their original photos with controlled access. Users can approve photo requests with a specified number of views and optional expiration time.

## Workflow

1. **User Registration**: Users register with sample images (128x128 downscaled versions)
2. **Photo Request**: Other users can request access to see the original photos
3. **Photo Approval**: Photo owners can approve/deny requests and set view limits
4. **Controlled Viewing**: Approved users can view originals up to the allowed count

## API Endpoints

### 1. Enhanced Registration
**POST** `/register`

```json
{
  "username": "alice",
  "password": "secret123",
  "sample_images": [
    "base64_encoded_128x128_image_1",
    "base64_encoded_128x128_image_2"
  ]
}
```

**Response:**
```json
{
  "status": "ok"
}
```

### 2. Request Photo Access
**POST** `/photo/request/{requester_username}`

```json
{
  "owner": "alice",
  "photo_id": "0",
  "message": "Hi Alice, can I see your original photo?"
}
```

**Response:**
```json
{
  "status": "ok",
  "request_id": "uuid-string",
  "message": "Photo access request sent"
}
```

### 3. Approve/Deny Photo Request
**POST** `/photo/approve/{owner_username}`

```json
{
  "request_id": "uuid-string",
  "approved": true,
  "max_views": 5,
  "expiry_hours": 24
}
```

**Response (Approved):**
```json
{
  "status": "approved",
  "message": "Access granted for 5 views"
}
```

**Response (Denied):**
```json
{
  "status": "denied",
  "message": "Access denied"
}
```

### 4. View Original Photo
**POST** `/photo/view/{requester_username}`

```json
{
  "request_id": "uuid-string"
}
```

**Response (Success):**
```json
{
  "success": true,
  "image_data": "base64_encoded_original_image",
  "views_remaining": 4,
  "message": null
}
```

**Response (Limit Exceeded):**
```json
{
  "success": false,
  "image_data": null,
  "views_remaining": 0,
  "message": "View limit exceeded"
}
```

### 5. List Pending Photo Requests (for owners)
**GET** `/photo/requests/{owner_username}`

**Response:**
```json
{
  "requests": [
    {
      "requester": "bob",
      "owner": "alice", 
      "photo_id": "0",
      "message": "Hi Alice, can I see your original photo?"
    }
  ]
}
```

### 6. List Access Records (for requesters)
**GET** `/photo/access/{requester_username}`

**Response:**
```json
{
  "access_records": [
    {
      "request_id": "uuid-string",
      "owner": "alice",
      "photo_id": "0",
      "max_views": 5,
      "current_views": 1,
      "views_remaining": 4,
      "created_at": "2025-12-02T10:30:00Z",
      "expires_at": "2025-12-03T10:30:00Z",
      "is_expired": false,
      "can_view": true
    }
  ]
}
```

### 7. Existing Endpoints (Enhanced)
- **POST** `/heartbeat` - Still tracks user presence
- **GET** `/discovery/online` - Lists online users with their sample images
- **GET** `/status` - Health check
- **GET** `/debug` - Debug information

## Data Storage

The server persists data in JSON files:

- `data/users.json` - User accounts with sample images
- `data/photo_requests.json` - Pending photo access requests
- `data/view_records.json` - Approved access records with view tracking

## View Count Rules

1. **View Limit**: Each approved request has a maximum number of views
2. **Expiration**: Optional expiry time (in hours from approval)
3. **One-time Consumption**: Each view decrements the remaining count
4. **Access Control**: Only the requester can view using their request_id
5. **Automatic Cleanup**: Expired or exhausted records are handled appropriately

## Security Features

- **Authentication**: Request validation against user credentials
- **Authorization**: Only owners can approve requests for their photos
- **Access Isolation**: Users can only view photos they've been granted access to
- **Rate Limiting**: Natural rate limiting through view count mechanism
- **Expiration**: Time-based access revocation

## Example Usage Flow

```bash
# 1. Alice registers with sample images
curl -X POST http://localhost:8000/register \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secret","sample_images":["base64..."]}' 

# 2. Bob requests access to Alice's photo #0
curl -X POST http://localhost:8000/photo/request/bob \
  -H "Content-Type: application/json" \
  -d '{"owner":"alice","photo_id":"0","message":"Can I see the original?"}'

# 3. Alice checks her pending requests
curl http://localhost:8000/photo/requests/alice

# 4. Alice approves Bob's request (5 views, 24h expiry)
curl -X POST http://localhost:8000/photo/approve/alice \
  -H "Content-Type: application/json" \
  -d '{"request_id":"uuid","approved":true,"max_views":5,"expiry_hours":24}'

# 5. Bob views the original photo (consumes 1 view)
curl -X POST http://localhost:8000/photo/view/bob \
  -H "Content-Type: application/json" \
  -d '{"request_id":"uuid"}'

# 6. Bob checks his remaining access
curl http://localhost:8000/photo/access/bob
```

## Integration with Heartbeat

The heartbeat mechanism can be enhanced to include view count updates:

```json
{
  "username": "bob",
  "ip": "192.168.1.100", 
  "port": 6000,
  "view_updates": [
    {
      "request_id": "uuid",
      "views_consumed": 1
    }
  ]
}
```

This allows for offline view tracking and synchronization during the next heartbeat.