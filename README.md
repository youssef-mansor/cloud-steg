# cloud-steg

**Run Command**
cargo run -- --config config.toml --this-node <ip>:<port>



**NEW**
# Terminal 1 - Node 1 (also runs HTTP API on port 3000)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3000 cargo run -- --config config.toml --this-node 127.0.0.1:8080

# Terminal 2 - Node 2 (HTTP API on port 3001)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3001 cargo run -- --config config.toml --this-node 127.0.0.1:8081

# Terminal 3 - Node 3 (HTTP API on port 3002)
SHARED_DRIVE_ID=0AEwep46IAWKDUk9PVA API_PORT=3002 cargo run -- --config config.toml --this-node 127.0.0.1:8082
