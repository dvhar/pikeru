#!/bin/bash
# This script installs embedding-server in /opt/embedding_server and creates a
# systemd service for it. It compiles the Rust project and installs the binary.

set -e

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "Please run as root (use sudo)"
    exit 1
fi

# Get relative path from git root to this directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PIKERU_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Determine install directory
INSTALL_DIR="/opt/embedding_server"
echo "Creating installation directory..."
mkdir -p "$INSTALL_DIR"

# Create service user
SERVICE_USER="embedding_service"
echo "Creating service user..."
if ! id "$SERVICE_USER" &>/dev/null; then
    useradd -r -s /bin/false -d "$INSTALL_DIR" "$SERVICE_USER"
fi

# Copy source files
echo "Copying source files to $INSTALL_DIR..."
cp -r "$PIKERU_ROOT/Cargo.toml" "$PIKERU_ROOT/src/" "$INSTALL_DIR/" 2>/dev/null || true
cp -r "$SCRIPT_DIR/Cargo.toml" "$SCRIPT_DIR/src/" "$INSTALL_DIR/"

# Build the project
echo "Building embedding-server..."
cd "$INSTALL_DIR"

# Ensure rustc and cargo are available
if ! command -v rustc &>/dev/null; then
    echo "Error: rustc not found. Install Rust via https://rustup.rs/"
    exit 1
fi

# If fastembed-rs is a sibling directory, we need to make the path work.
# Check if it's already absolute or relative
if [ -d "$PIKERU_ROOT/../../fastembed-rs" ]; then
    FASTEMBED_PATH="$PIKERU_ROOT/../../fastembed-rs"
elif [ -d "/home/d/gits/fastembed-rs" ]; then
    FASTEMBED_PATH="/home/d/gits/fastembed-rs"
else
    echo "Warning: fastembed-rs not found at expected paths."
    echo "Make sure to update the path in Cargo.toml before building."
    exit 1
fi

# Update the fastembed path in Cargo.toml to be absolute (since we're in /opt)
sed -i "s|path = \"../../../fastembed-rs\"|path = \"$FASTEMBED_PATH\"|" "$INSTALL_DIR/Cargo.toml"

echo "Building with fastembed at: $FASTEMBED_PATH"
cargo build --release 2>&1

if [ ! -f "target/release/embedding-server" ]; then
    echo "Error: Build failed — target/release/embedding-server not found"
    exit 1
fi

# Install binary
cp "target/release/embedding-server" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/embedding-server"

# Create systemd service file
echo "Creating systemd service..."
cat > /etc/systemd/system/embedding-server.service << EOL
[Unit]
Description=Pikeru Embedding Server (semantic search)
After=network.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
Environment=HOME=/opt/embedding_server
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/embedding-server --serve
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOL

# Set permissions
echo "Setting permissions..."
chown -R $SERVICE_USER:$SERVICE_USER "$INSTALL_DIR"

# Reload systemd and enable service
echo "Enabling and starting service..."
systemctl daemon-reload
systemctl enable embedding-server
systemctl start embedding-server

echo ""
echo "Installation complete!"
echo "  Service:    systemctl status embedding-server"
echo "  Logs:       journalctl -u embedding-server -f"
echo "  Endpoint:   http://127.0.0.1:6285/health"
