#!/bin/bash
# This script installs caption server in /opt/caption_server and creates a
# systemd service for it.

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "Please run as root (use sudo)"
    exit 1
fi

# Check if we're in a git repository
if ! git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    echo "Error: Not inside a git repository"
    exit 1
fi

# Get the current directory name
CURRENT_DIR=$(basename $(pwd))

# Get relative path from git root to current directory
REL_PATH=$(git rev-parse --show-prefix)

# Determine if we're in the caption_server directory or need to look for it
if [ "$CURRENT_DIR" = "caption_server" ]; then
    # We're already in the caption_server directory
    SOURCE_PREFIX=""
    SOURCE_PATH="."
else
    # Check if caption_server exists in current directory
    if [ ! -d "caption_server" ]; then
        echo "Error: Must be run from within caption_server directory or from its parent"
        exit 1
    fi
    SOURCE_PREFIX="caption_server/"
    SOURCE_PATH="caption_server"
fi

# Create installation directory
INSTALL_DIR="/opt/caption_server"
HOME_DIR="$INSTALL_DIR/home"
echo "Creating installation directory..."
mkdir -p $INSTALL_DIR
mkdir -p $HOME_DIR

# Create service user
SERVICE_USER="caption_service"
echo "Creating service user..."
useradd -r -s /bin/false -d $HOME_DIR $SERVICE_USER

# Copy git-tracked files to installation directory
echo "Copying tracked files to $INSTALL_DIR..."
git ls-files "$SOURCE_PATH" | while read file; do
    # Remove the source prefix if it exists
    dest_file=${file#$SOURCE_PREFIX}
    # Create the directory structure if it doesn't exist
    install -D "$file" "$INSTALL_DIR/$dest_file"
done

# Create virtual environment and install dependencies
echo "Setting up Python virtual environment..."
python3 -m venv $INSTALL_DIR/venv
source $INSTALL_DIR/venv/bin/activate
pip install -r $INSTALL_DIR/requirements.txt
deactivate

# Ensure start.sh is executable
chmod +x $INSTALL_DIR/start.sh

# Create systemd service file
echo "Creating systemd service..."
cat > /etc/systemd/system/caption-server.service << EOL
[Unit]
Description=Caption Generator Server
After=network.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
Environment=HOME=$HOME_DIR
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/start.sh
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOL

# Set correct permissions
echo "Setting permissions..."
chown -R $SERVICE_USER:$SERVICE_USER $INSTALL_DIR
chmod -R 755 $INSTALL_DIR

# Reload systemd daemon and enable service
echo "Enabling and starting service..."
systemctl daemon-reload
systemctl enable caption-server
systemctl start caption-server

echo "Installation complete!"
echo "You can check the service status with: systemctl status caption-server"
