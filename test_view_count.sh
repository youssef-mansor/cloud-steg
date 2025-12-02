#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

BASE_URL="http://localhost:8000"

# Sample base64 encoded images (small placeholders for testing)
IMAGE1="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
IMAGE2="iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFklEQVQImWP4z8DwHwMYGBgY/hMBGZADRJn2FkkAAAAASUVORK5CYII="

echo -e "${BLUE}=== P2P Photo Sharing View Count Test ===${NC}\n"

# Test 1: Register users
echo -e "${YELLOW}Test 1: Register Alice with sample images${NC}"
ALICE_RESPONSE=$(curl -s -X POST "$BASE_URL/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"alice\",
    \"password\": \"alice123\",
    \"sample_images\": [\"$IMAGE1\", \"$IMAGE2\"]
  }")
echo "Response: $ALICE_RESPONSE"
echo ""

echo -e "${YELLOW}Test 2: Register Bob with sample images${NC}"
BOB_RESPONSE=$(curl -s -X POST "$BASE_URL/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"bob\",
    \"password\": \"bob123\",
    \"sample_images\": [\"$IMAGE1\"]
  }")
echo "Response: $BOB_RESPONSE"
echo ""

# Test 3: Heartbeat to make users online
echo -e "${YELLOW}Test 3: Alice sends heartbeat${NC}"
HEARTBEAT_ALICE=$(curl -s -X POST "$BASE_URL/heartbeat" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"alice\",
    \"ip\": \"192.168.1.10\",
    \"port\": 6000
  }")
echo "Response: $HEARTBEAT_ALICE"
echo ""

echo -e "${YELLOW}Test 4: Bob sends heartbeat${NC}"
HEARTBEAT_BOB=$(curl -s -X POST "$BASE_URL/heartbeat" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"bob\",
    \"ip\": \"192.168.1.20\",
    \"port\": 6001
  }")
echo "Response: $HEARTBEAT_BOB"
echo ""

# Test 4: Bob requests access to Alice's photo
echo -e "${YELLOW}Test 5: Bob requests access to Alice's photo #0${NC}"
REQUEST_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/request/bob" \
  -H "Content-Type: application/json" \
  -d "{
    \"owner\": \"alice\",
    \"photo_id\": \"0\",
    \"message\": \"Hi Alice, can I see your beautiful photo?\"
  }")
echo "Response: $REQUEST_RESPONSE"
REQUEST_ID=$(echo "$REQUEST_RESPONSE" | grep -o '"request_id":"[^"]*' | cut -d'"' -f4)
echo -e "${GREEN}Request ID: $REQUEST_ID${NC}\n"

# Test 5: Alice checks her pending requests
echo -e "${YELLOW}Test 6: Alice checks her pending photo requests${NC}"
PENDING=$(curl -s "$BASE_URL/photo/requests/alice")
echo "Response: $PENDING"
echo ""

# Test 6: Alice approves Bob's request with 3 views and 2-hour expiry
echo -e "${YELLOW}Test 7: Alice approves Bob's request (3 views, 2-hour expiry)${NC}"
APPROVE_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/approve/alice" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID\",
    \"approved\": true,
    \"max_views\": 3,
    \"expiry_hours\": 2
  }")
echo "Response: $APPROVE_RESPONSE"
echo ""

# Test 7: Bob checks his access records
echo -e "${YELLOW}Test 8: Bob checks his access permissions${NC}"
ACCESS=$(curl -s "$BASE_URL/photo/access/bob")
echo "Response: $ACCESS"
echo ""

# Test 8: Bob views the photo (first view)
echo -e "${YELLOW}Test 9: Bob views Alice's photo (1st view)${NC}"
VIEW1=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID\"
  }")
echo "Response: $VIEW1"
VIEWS_LEFT=$(echo "$VIEW1" | grep -o '"views_remaining":[0-9]*' | cut -d':' -f2)
echo -e "${GREEN}Views remaining: $VIEWS_LEFT${NC}\n"

# Test 9: Bob views the photo again (second view)
echo -e "${YELLOW}Test 10: Bob views Alice's photo (2nd view)${NC}"
VIEW2=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID\"
  }")
echo "Response: $VIEW2"
VIEWS_LEFT=$(echo "$VIEW2" | grep -o '"views_remaining":[0-9]*' | cut -d':' -f2)
echo -e "${GREEN}Views remaining: $VIEWS_LEFT${NC}\n"

# Test 10: Bob views the photo third time (third view)
echo -e "${YELLOW}Test 11: Bob views Alice's photo (3rd view)${NC}"
VIEW3=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID\"
  }")
echo "Response: $VIEW3"
VIEWS_LEFT=$(echo "$VIEW3" | grep -o '"views_remaining":[0-9]*' | cut -d':' -f2)
echo -e "${GREEN}Views remaining: $VIEWS_LEFT${NC}\n"

# Test 11: Bob tries to view the photo fourth time (should fail)
echo -e "${YELLOW}Test 12: Bob tries to view Alice's photo (4th view - should fail)${NC}"
VIEW4=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID\"
  }")
echo "Response: $VIEW4"
echo -e "${RED}(Expected to fail - view limit exceeded)${NC}\n"

# Test 12: Bob checks his access records again
echo -e "${YELLOW}Test 13: Bob checks his updated access permissions${NC}"
ACCESS_UPDATED=$(curl -s "$BASE_URL/photo/access/bob")
echo "Response: $ACCESS_UPDATED"
echo ""

# Test 13: Test photo denial
echo -e "${YELLOW}Test 14: Charlie requests access to Alice's photo${NC}"
REQ2=$(curl -s -X POST "$BASE_URL/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"charlie\",
    \"password\": \"charlie123\",
    \"sample_images\": [\"$IMAGE1\"]
  }")

REQUEST2_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/request/charlie" \
  -H "Content-Type: application/json" \
  -d "{
    \"owner\": \"alice\",
    \"photo_id\": \"1\",
    \"message\": \"I also want to see photo #1\"
  }")
REQUEST_ID_2=$(echo "$REQUEST2_RESPONSE" | grep -o '"request_id":"[^"]*' | cut -d'"' -f4)
echo -e "${GREEN}Request ID: $REQUEST_ID_2${NC}\n"

echo -e "${YELLOW}Test 15: Alice denies Charlie's request${NC}"
DENY_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/approve/alice" \
  -H "Content-Type: application/json" \
  -d "{
    \"request_id\": \"$REQUEST_ID_2\",
    \"approved\": false,
    \"max_views\": 0
  }")
echo "Response: $DENY_RESPONSE"
echo ""

# Test 14: Check discovery to see online users
echo -e "${YELLOW}Test 16: Discovery check - see all online users${NC}"
DISCOVERY=$(curl -s "$BASE_URL/discovery/online")
echo "Response: $DISCOVERY"
echo ""

echo -e "${GREEN}=== All tests completed! ===${NC}"
