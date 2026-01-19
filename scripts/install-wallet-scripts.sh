#!/bin/bash
# Installs wallet operation scripts to /opt/solana/bin
# This script must be run with sudo privileges
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="/opt/solana/bin"

echo "=== Installing Wallet Scripts ==="
echo ""

# Verify running as root
if [[ $EUID -ne 0 ]]; then
   echo "ERROR: This script must be run with sudo"
   exit 1
fi

# Verify source scripts exist
for script in wallet_show_address.sh wallet_balance.sh wallet_recent_txs.sh; do
    if [[ ! -f "$SCRIPT_DIR/$script" ]]; then
        echo "ERROR: Source script not found: $SCRIPT_DIR/$script"
        exit 1
    fi
done

# Create install directory if needed
mkdir -p "$INSTALL_DIR"

# Copy and set permissions
echo "Installing scripts to $INSTALL_DIR..."
cp "$SCRIPT_DIR/wallet_show_address.sh" "$INSTALL_DIR/"
cp "$SCRIPT_DIR/wallet_balance.sh" "$INSTALL_DIR/"
cp "$SCRIPT_DIR/wallet_recent_txs.sh" "$INSTALL_DIR/"

# Set ownership and permissions
chown solbot:solbot "$INSTALL_DIR"/*.sh
chmod 755 "$INSTALL_DIR"/*.sh

echo ""
echo "Installed scripts:"
ls -la "$INSTALL_DIR"/*.sh

echo ""
echo "=== Installation Complete ==="
echo ""
echo "Usage:"
echo "  sudo -u solbot $INSTALL_DIR/wallet_show_address.sh"
echo "  sudo -u solbot $INSTALL_DIR/wallet_balance.sh"
echo "  sudo -u solbot $INSTALL_DIR/wallet_recent_txs.sh [limit]"
