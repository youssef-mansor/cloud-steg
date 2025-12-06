#!/bin/bash
# Download all images from online clients
# Usage: ./download_online_clients.sh [LEADER_URL]

set -e  # Exit on error

LEADER_URL="${1:-http://10.40.6.26:3000}"
OUTPUT_DIR="./online_clients"
ENDPOINT="$LEADER_URL/discover_with_images"

echo "==================================="
echo "Downloading Online Clients & Images"
echo "==================================="
echo "Leader: $LEADER_URL"
echo "Output: $OUTPUT_DIR"
echo ""

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    echo "Error: 'jq' is required but not installed."
    echo "Install with: sudo apt-get install jq"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Download JSON
echo "Fetching data from $ENDPOINT..."
RESPONSE=$(curl -s "$ENDPOINT")

# Check if we're the leader
IS_LEADER=$(echo "$RESPONSE" | jq -r '.count // 0')
if [ "$IS_LEADER" = "0" ]; then
    echo "Error: No online clients or not connected to leader"
    exit 1
fi

# Parse and save images
echo "$RESPONSE" | jq -c '.online_clients[]' | while read -r client; do
    USERNAME=$(echo "$client" | jq -r '.username')
    ADDR=$(echo "$client" | jq -r '.addr')
    IMAGE_COUNT=$(echo "$client" | jq -r '.images | length')
    
    echo ""
    echo "Client: $USERNAME @ $ADDR"
    echo "  Images: $IMAGE_COUNT"
    
    # Create user directory
    USER_DIR="$OUTPUT_DIR/$USERNAME"
    mkdir -p "$USER_DIR"
    
    # Save user info
    echo "$client" | jq '{username, addr}' > "$USER_DIR/info.json"
    
    # Extract and decode images
    if [ "$IMAGE_COUNT" -gt 0 ]; then
        echo "$client" | jq -c '.images[]' | while read -r image; do
            FILENAME=$(echo "$image" | jq -r '.filename')
            DATA=$(echo "$image" | jq -r '.data')
            
            # Decode base64 and save
            echo "$DATA" | base64 -d > "$USER_DIR/$FILENAME"
            echo "    ✓ $FILENAME"
        done
    else
        echo "    (no images)"
    fi
done

echo ""
echo "==================================="
echo "Download complete!"
echo "Files saved to: $OUTPUT_DIR/"
echo "==================================="

# Show summary
echo ""
echo "Summary:"
find "$OUTPUT_DIR" -type d -mindepth 1 -maxdepth 1 | while read -r dir; do
    USERNAME=$(basename "$dir")
    IMAGE_COUNT=$(find "$dir" -type f -name "*.png" -o -name "*.jpg" -o -name "*.webp" | wc -l)
    echo "  $USERNAME: $IMAGE_COUNT images"
done
