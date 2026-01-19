#!/bin/bash
# Secure mainnet wallet setup script for kyzlo-dex trading bot
# This script must be run with sudo privileges
set -euo pipefail

SOLANA_BIN="${SOLANA_BIN:-/home/ubuntu/.local/share/solana/install/releases/2.1.21/solana-release/bin}"
WALLET_DIR="/opt/solana/wallets/trading"
KEYPAIR_PATH="$WALLET_DIR/id.json"
BIN_DIR="/opt/solana/bin"

echo "=== Kyzlo-DEX Mainnet Wallet Setup ==="
echo ""

# Verify running as root
if [[ $EUID -ne 0 ]]; then
   echo "ERROR: This script must be run with sudo"
   exit 1
fi

# Verify solbot user exists
if ! id solbot &>/dev/null; then
    echo "ERROR: solbot user does not exist. Run first:"
    echo "  sudo useradd -r -s /bin/false -d /opt/solana solbot"
    exit 1
fi

# Verify solana-keygen exists
if [[ ! -x "$SOLANA_BIN/solana-keygen" ]]; then
    echo "ERROR: solana-keygen not found at $SOLANA_BIN"
    echo "Set SOLANA_BIN environment variable to correct path"
    exit 1
fi

echo "[1/4] Creating directory structure..."
mkdir -p "$WALLET_DIR"
mkdir -p "$BIN_DIR"
chown -R solbot:solbot /opt/solana
chmod 700 "$WALLET_DIR"

echo "[2/4] Generating mainnet keypair..."
# Generate keypair - suppress any secret output
"$SOLANA_BIN/solana-keygen" new \
    --outfile "$KEYPAIR_PATH" \
    --no-bip39-passphrase \
    --force \
    --silent 2>/dev/null

# Set strict permissions
chown solbot:solbot "$KEYPAIR_PATH"
chmod 600 "$KEYPAIR_PATH"

echo "[3/4] Verifying permissions..."
ls -la "$KEYPAIR_PATH"

echo ""
echo "[4/4] Wallet public address (SAFE to share):"
"$SOLANA_BIN/solana-keygen" pubkey "$KEYPAIR_PATH"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "IMPORTANT: The wallet is UNFUNDED. Send SOL to the address above before trading."
echo "Keypair location: $KEYPAIR_PATH"
echo "Permissions: 600 (owner read/write only)"
echo ""
echo "NEVER share or display the contents of $KEYPAIR_PATH"
