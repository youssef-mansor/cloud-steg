#!/bin/bash

# Comprehensive Test Script for P2P Discovery & Photo Sharing System
# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE_URL="http://localhost:8000"
SERVER_BIN="$PROJECT_ROOT/server/target/release/p2p-discovery-server"
CLIENT_BIN="$PROJECT_ROOT/client/target/release/p2p-client"

# Test results tracking
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

# Function to print test header
print_test() {
    echo -e "\n${BLUE}=== $1 ===${NC}"
}

# Function to print success
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
    ((TESTS_PASSED++))
}

# Function to print failure
print_failure() {
    echo -e "${RED}✗ $1${NC}"
    ((TESTS_FAILED++))
    FAILED_TESTS+=("$1")
}

# Function to check HTTP response
check_response() {
    local response="$1"
    local expected_status="$2"
    local test_name="$3"
    
    if echo "$response" | grep -q "$expected_status"; then
        print_success "$test_name"
        return 0
    else
        print_failure "$test_name"
        echo "Response: $response"
        return 1
    fi
}

# Clean up old data
echo -e "${YELLOW}Cleaning up old test data...${NC}"
rm -f "$PROJECT_ROOT/data/users.json"
rm -f "$PROJECT_ROOT/data/photo_requests.json"
rm -f "$PROJECT_ROOT/data/view_records.json"
rm -f "$PROJECT_ROOT/data/client_*.json"

# Start server
echo -e "${YELLOW}Starting server...${NC}"
cd "$PROJECT_ROOT/server"
"$SERVER_BIN" > /tmp/p2p_server.log 2>&1 &
SERVER_PID=$!
cd "$PROJECT_ROOT"

# Wait for server to start
echo "Waiting for server to start..."
sleep 3

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo -e "${RED}ERROR: Server failed to start${NC}"
    cat /tmp/p2p_server.log
    exit 1
fi

# Test server health
print_test "Test 1: Server Health Check"
HEALTH=$(curl -s "$BASE_URL/status")
if echo "$HEALTH" | grep -q "ok"; then
    print_success "Server is running"
else
    print_failure "Server health check failed"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

# Test 3: User Registration
print_test "Test 3: User Registration"

# Register Alice without images (we'll test image registration separately if needed)
ALICE_REG=$(curl -s -X POST "$BASE_URL/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"alice\",
        \"password\": \"alice123\",
        \"sample_images\": []
    }")
check_response "$ALICE_REG" "ok" "Alice registration"

# Register Bob
BOB_REG=$(curl -s -X POST "$BASE_URL/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"bob\",
        \"password\": \"bob123\",
        \"sample_images\": []
    }")
check_response "$BOB_REG" "ok" "Bob registration"

# Try duplicate registration
DUPLICATE_REG=$(curl -s -X POST "$BASE_URL/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"alice\",
        \"password\": \"different\",
        \"sample_images\": []
    }")
if echo "$DUPLICATE_REG" | grep -q "exists"; then
    print_success "Duplicate registration rejected"
else
    print_failure "Duplicate registration should be rejected"
fi

# Test 4: Heartbeat System
print_test "Test 4: Heartbeat System"

ALICE_HEARTBEAT=$(curl -s -X POST "$BASE_URL/heartbeat" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"alice\",
        \"ip\": \"192.168.1.10\",
        \"port\": 6000
    }")
check_response "$ALICE_HEARTBEAT" "ok" "Alice heartbeat"

BOB_HEARTBEAT=$(curl -s -X POST "$BASE_URL/heartbeat" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"bob\",
        \"ip\": \"192.168.1.20\",
        \"port\": 6001
    }")
check_response "$BOB_HEARTBEAT" "ok" "Bob heartbeat"

# Test 5: Discovery/Online Users
print_test "Test 5: Discovery - Online Users"

DISCOVERY=$(curl -s "$BASE_URL/discovery/online")
if echo "$DISCOVERY" | grep -q "alice" && echo "$DISCOVERY" | grep -q "bob"; then
    print_success "Discovery shows both users online"
    echo "Discovery response: $DISCOVERY"
else
    print_failure "Discovery missing users"
    echo "Discovery response: $DISCOVERY"
fi

# Test 6: Photo Request Workflow
print_test "Test 6: Photo Request Workflow"

# Bob requests access to Alice's photo
REQUEST_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/request/bob" \
    -H "Content-Type: application/json" \
    -d "{
        \"owner\": \"alice\",
        \"photo_id\": \"0\",
        \"message\": \"Hi Alice, can I see your photo?\"
    }")
REQUEST_ID=$(echo "$REQUEST_RESPONSE" | grep -o '"request_id":"[^"]*' | cut -d'"' -f4)

if [ -n "$REQUEST_ID" ]; then
    print_success "Photo request created (ID: $REQUEST_ID)"
else
    print_failure "Photo request failed"
    echo "Response: $REQUEST_RESPONSE"
fi

# Alice checks pending requests
PENDING=$(curl -s "$BASE_URL/photo/requests/alice")
if echo "$PENDING" | grep -q "bob"; then
    print_success "Alice sees pending request"
else
    print_failure "Pending requests not visible"
    echo "Response: $PENDING"
fi

# Test 7: Photo Approval
print_test "Test 7: Photo Approval"

if [ -n "$REQUEST_ID" ]; then
    APPROVE_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/approve/alice" \
        -H "Content-Type: application/json" \
        -d "{
            \"request_id\": \"$REQUEST_ID\",
            \"approved\": true,
            \"max_views\": 3,
            \"expiry_hours\": 2
        }")
    if echo "$APPROVE_RESPONSE" | grep -q "approved"; then
        print_success "Photo request approved"
    else
        print_failure "Photo approval failed"
        echo "Response: $APPROVE_RESPONSE"
    fi
fi

# Test 8: View Count Mechanism
print_test "Test 8: View Count Mechanism"

if [ -n "$REQUEST_ID" ]; then
    # Bob checks access records
    ACCESS=$(curl -s "$BASE_URL/photo/access/bob")
    if echo "$ACCESS" | grep -q "$REQUEST_ID"; then
        print_success "Bob sees access record"
    else
        print_failure "Access record not found"
        echo "Response: $ACCESS"
    fi
    
    # Try to view photo (should work first 3 times)
    for i in {1..3}; do
        VIEW_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
            -H "Content-Type: application/json" \
            -d "{
                \"request_id\": \"$REQUEST_ID\"
            }")
        VIEWS_REMAINING=$(echo "$VIEW_RESPONSE" | grep -o '"views_remaining":[0-9]*' | cut -d':' -f2)
        
        if [ "$i" -le 3 ]; then
            if echo "$VIEW_RESPONSE" | grep -q "success.*true"; then
                print_success "View $i successful (views remaining: $VIEWS_REMAINING)"
            else
                print_failure "View $i should succeed"
                echo "Response: $VIEW_RESPONSE"
            fi
        fi
    done
    
    # 4th view should fail
    VIEW4=$(curl -s -X POST "$BASE_URL/photo/view/bob" \
        -H "Content-Type: application/json" \
        -d "{
            \"request_id\": \"$REQUEST_ID\"
        }")
    if echo "$VIEW4" | grep -q "limit exceeded\|success.*false"; then
        print_success "4th view correctly rejected (limit exceeded)"
    else
        print_failure "4th view should be rejected"
        echo "Response: $VIEW4"
    fi
fi

# Test 9: Request Denial
print_test "Test 9: Request Denial"

# Register Charlie
CHARLIE_REG=$(curl -s -X POST "$BASE_URL/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"charlie\",
        \"password\": \"charlie123\",
        \"sample_images\": []
    }")

# Charlie requests access
REQ2_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/request/charlie" \
    -H "Content-Type: application/json" \
    -d "{
        \"owner\": \"alice\",
        \"photo_id\": \"0\",
        \"message\": \"I also want access\"
    }")
REQUEST_ID_2=$(echo "$REQ2_RESPONSE" | grep -o '"request_id":"[^"]*' | cut -d'"' -f4)

if [ -n "$REQUEST_ID_2" ]; then
    # Alice denies the request
    DENY_RESPONSE=$(curl -s -X POST "$BASE_URL/photo/approve/alice" \
        -H "Content-Type: application/json" \
        -d "{
            \"request_id\": \"$REQUEST_ID_2\",
            \"approved\": false,
            \"max_views\": 0
        }")
    if echo "$DENY_RESPONSE" | grep -q "denied"; then
        print_success "Request denial works"
    else
        print_failure "Request denial failed"
        echo "Response: $DENY_RESPONSE"
    fi
fi

# Test 10: Client CLI Commands
print_test "Test 10: Client CLI Commands"

# Test list-online command
CLI_DISCOVERY=$("$CLIENT_BIN" list-online --server "$BASE_URL" 2>&1)
if echo "$CLI_DISCOVERY" | grep -q "alice\|bob"; then
    print_success "CLI list-online works"
else
    print_failure "CLI list-online failed"
    echo "Output: $CLI_DISCOVERY"
fi

# Test 11: TTL Expiration (simulate)
print_test "Test 11: TTL Expiration Test"

# Send heartbeat for test user
TEST_USER_HEARTBEAT=$(curl -s -X POST "$BASE_URL/heartbeat" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"testuser\",
        \"ip\": \"192.168.1.30\",
        \"port\": 6002
    }")

# Check if user appears online
DISCOVERY_AFTER=$(curl -s "$BASE_URL/discovery/online")
if echo "$DISCOVERY_AFTER" | grep -q "testuser"; then
    print_success "User appears online after heartbeat"
else
    print_failure "User not appearing online"
fi

# Summary
echo -e "\n${BLUE}========================================${NC}"
echo -e "${BLUE}Test Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Tests Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Tests Failed: $TESTS_FAILED${NC}"

if [ $TESTS_FAILED -gt 0 ]; then
    echo -e "\n${RED}Failed Tests:${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo -e "  - $test"
    done
fi

# Cleanup
echo -e "\n${YELLOW}Cleaning up...${NC}"
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null || true

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed. Check output above.${NC}"
    exit 1
fi

