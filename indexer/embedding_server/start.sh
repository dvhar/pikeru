#!/bin/bash
# Start embedding-server in the foreground (for manual testing or systemd).
cd "$(dirname $0)"

if [ ! -f "./embedding-server" ]; then
    echo "Error: embedding-server binary not found. Run install.sh first, or build with:"
    echo "  cargo build --release && cp target/release/embedding-server ."
    exit 1
fi

exec ./embedding-server --serve
