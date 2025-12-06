#!/bin/bash
# Quick Reference: Copy-paste these commands one by one to test each feature

BASE_URL="http://localhost:8000"

echo "=========================================="
echo "P2P Discovery & Photo Sharing Test Commands"
echo "=========================================="
echo ""
echo "Make sure the server is running first!"
echo "cd server && cargo run --release"
echo ""
echo "=========================================="
echo ""

# Test 1: Health Check
echo "1. Health Check:"
echo "curl $BASE_URL/status"
echo ""

# Test 2: Registration
echo "2. Register Alice:"
echo "curl -X POST $BASE_URL/register -H 'Content-Type: application/json' -d '{\"username\":\"alice\",\"password\":\"alice123\",\"sample_images\":[]}'"
echo ""

echo "3. Register Bob:"
echo "curl -X POST $BASE_URL/register -H 'Content-Type: application/json' -d '{\"username\":\"bob\",\"password\":\"bob123\",\"sample_images\":[]}'"
echo ""

# Test 3: Heartbeat
echo "4. Alice heartbeat:"
echo "curl -X POST $BASE_URL/heartbeat -H 'Content-Type: application/json' -d '{\"username\":\"alice\",\"ip\":\"192.168.1.10\",\"port\":6000}'"
echo ""

echo "5. Bob heartbeat:"
echo "curl -X POST $BASE_URL/heartbeat -H 'Content-Type: application/json' -d '{\"username\":\"bob\",\"ip\":\"192.168.1.20\",\"port\":6001}'"
echo ""

# Test 4: Discovery
echo "6. List online users:"
echo "curl $BASE_URL/discovery/online"
echo ""

# Test 5: Photo Request
echo "7. Bob requests Alice's photo:"
echo "curl -X POST $BASE_URL/photo/request/bob -H 'Content-Type: application/json' -d '{\"owner\":\"alice\",\"photo_id\":\"0\",\"message\":\"Can I see your photo?\"}'"
echo ""
echo "   (Save the request_id from the response!)"
echo ""

echo "8. Alice checks pending requests:"
echo "curl $BASE_URL/photo/requests/alice"
echo ""

# Test 6: Approval
echo "9. Alice approves request (replace REQUEST_ID):"
echo "curl -X POST $BASE_URL/photo/approve/alice -H 'Content-Type: application/json' -d '{\"request_id\":\"REQUEST_ID\",\"approved\":true,\"max_views\":3,\"expiry_hours\":24}'"
echo ""

echo "10. Bob checks access records:"
echo "curl $BASE_URL/photo/access/bob"
echo ""

# Test 7: View Photo
echo "11. Bob views photo (replace REQUEST_ID):"
echo "curl -X POST $BASE_URL/photo/view/bob -H 'Content-Type: application/json' -d '{\"request_id\":\"REQUEST_ID\"}'"
echo ""

echo "12. View again (2nd time):"
echo "curl -X POST $BASE_URL/photo/view/bob -H 'Content-Type: application/json' -d '{\"request_id\":\"REQUEST_ID\"}'"
echo ""

echo "13. View again (3rd time):"
echo "curl -X POST $BASE_URL/photo/view/bob -H 'Content-Type: application/json' -d '{\"request_id\":\"REQUEST_ID\"}'"
echo ""

echo "14. Try 4th view (should fail):"
echo "curl -X POST $BASE_URL/photo/view/bob -H 'Content-Type: application/json' -d '{\"request_id\":\"REQUEST_ID\"}'"
echo ""

# Test 8: CLI
echo "15. Use CLI to list online:"
echo "./client/target/release/p2p-client list-online --server $BASE_URL"
echo ""

echo "=========================================="
echo "See MANUAL_TEST_COMMANDS.md for detailed guide"
echo "=========================================="

