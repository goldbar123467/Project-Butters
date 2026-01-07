#!/bin/bash
# Install Butters Trading Bot as a systemd service
# Usage: sudo ./scripts/install-service.sh

set -euo pipefail

SERVICE_NAME="butters"
SERVICE_FILE="/home/ubuntu/projects/kyzlo-dex/butters.service"
SYSTEMD_DIR="/etc/systemd/system"

echo "=== Installing Butters Trading Bot Service ==="
echo ""

# Check if running as root
if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (use sudo)"
    exit 1
fi

# Check if service file exists
if [[ ! -f "$SERVICE_FILE" ]]; then
    echo "ERROR: Service file not found at $SERVICE_FILE"
    exit 1
fi

# Check if binary exists
if [[ ! -f "/home/ubuntu/projects/kyzlo-dex/target/release/butters" ]]; then
    echo "WARNING: Binary not found at /home/ubuntu/projects/kyzlo-dex/target/release/butters"
    echo "         Make sure to build with: cargo build --release"
    echo ""
fi

# Check if config exists
if [[ ! -f "/home/ubuntu/projects/kyzlo-dex/config.toml" ]]; then
    echo "WARNING: Config file not found at /home/ubuntu/projects/kyzlo-dex/config.toml"
    echo ""
fi

# Copy service file
echo "Copying service file to $SYSTEMD_DIR..."
cp "$SERVICE_FILE" "$SYSTEMD_DIR/${SERVICE_NAME}.service"

# Reload systemd daemon
echo "Reloading systemd daemon..."
systemctl daemon-reload

echo ""
echo "=== Installation Complete ==="
echo ""
echo "Service management commands:"
echo ""
echo "  Start the service:"
echo "    sudo systemctl start ${SERVICE_NAME}"
echo ""
echo "  Stop the service:"
echo "    sudo systemctl stop ${SERVICE_NAME}"
echo ""
echo "  Check service status:"
echo "    sudo systemctl status ${SERVICE_NAME}"
echo ""
echo "  Enable auto-start on boot:"
echo "    sudo systemctl enable ${SERVICE_NAME}"
echo ""
echo "  View live logs:"
echo "    journalctl -u ${SERVICE_NAME} -f"
echo ""
echo "  View recent logs:"
echo "    journalctl -u ${SERVICE_NAME} -n 100"
echo ""
echo "  Restart the service:"
echo "    sudo systemctl restart ${SERVICE_NAME}"
echo ""
