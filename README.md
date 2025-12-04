# cloud-steg

**Run Command**
cargo run -- --config config.toml --this-node <ip>:<port>

peers = [
 "10.40.54.163:5000",
 "10.40.51.185:5000",
 "10.40.44.249:5000",
]


**NEW**
# Terminal 1 - Node 1 (also runs HTTP API on port 3000)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA REGISTERED_USERS_FOLDER_ID=1P5o3QM-PicdKdYNQM9YRqaNje4fv6QNk API_PORT=3000 cargo run -- --config config.toml --this-node 10.40.44.249:8080

# Terminal 2 - Node 2 (HTTP API on port 3001)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3001 cargo run -- --config config.toml --this-node 10.40.51.185:8081

# Terminal 3 - Node 3 (HTTP API on port 3002)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3002 cargo run -- --config config.toml --this-node 127.0.0.1:8082


**Register Service**
curl -X POST http://10.40.7.1:3000/register \
  -H "Content-Type: application/json" \
  -d '{"username": "youssef", "addr": "192.168.1.50:6666"}'


**Heartbeat Test**
curl http://10.40.44.249:3000/

# Response: {"success": true, "message": "Heartbeat accepted for 'alice'"}

# Send heartbeat as "bob"
curl -X POST http://10.40.6.26:3000/heartbeat \
  -H "Content-Type: application/json" \
  -d '{"username": "bob"}'

# Check status to see online count
curl http://10.40.44.249:3000/

# Response will show: "online_clients_count": 2

## bullshit

# Terminal 1: Alice heartbeats
while true; do
  curl -X POST http://10.40.6.26:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "alice"}' \
    --silent --output /dev/null
  sleep 8
done &

# Terminal 2: Bob heartbeats  
while true; do
  curl -X POST http://10.40.6.26:3000/heartbeat \
    -H "Content-Type: application/json" \
    -d '{"username": "bob"}' \
    --silent --output /dev/null
  sleep 12
done &
