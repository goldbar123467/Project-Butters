#!/bin/bash
# Butters Trading Bot - Devnet Setup Script
# This script sets up your Solana environment for devnet testing

set -e

DEVNET_KEYPAIR="$HOME/.config/solana/devnet.json"
DEVNET_RPC="https://api.devnet.solana.com"

echo "========================================"
echo "  Butters Trading Bot - Devnet Setup"
echo "========================================"
echo ""

# Step 1: Check if Solana CLI is installed
echo "[1/5] Checking Solana CLI installation..."
if ! command -v solana &> /dev/null; then
    echo ""
    echo "ERROR: Solana CLI is not installed."
    echo ""
    echo "Install it using one of these methods:"
    echo ""
    echo "  Option 1 - Official installer (recommended):"
    echo "    sh -c \"\$(curl -sSfL https://release.anza.xyz/stable/install)\""
    echo ""
    echo "  Option 2 - Homebrew (macOS):"
    echo "    brew install solana"
    echo ""
    echo "After installation, add to PATH and restart your terminal:"
    echo "    export PATH=\"\$HOME/.local/share/solana/install/active_release/bin:\$PATH\""
    echo ""
    exit 1
fi

SOLANA_VERSION=$(solana --version)
echo "  Found: $SOLANA_VERSION"
echo ""

# Step 2: Create devnet keypair if it doesn't exist
echo "[2/5] Setting up devnet keypair..."
if [ -f "$DEVNET_KEYPAIR" ]; then
    echo "  Keypair already exists at: $DEVNET_KEYPAIR"
else
    echo "  Creating new keypair at: $DEVNET_KEYPAIR"
    solana-keygen new --outfile "$DEVNET_KEYPAIR" --no-bip39-passphrase
fi
echo ""

# Step 3: Configure Solana CLI for devnet
echo "[3/5] Configuring Solana CLI for devnet..."
solana config set --url "$DEVNET_RPC" --keypair "$DEVNET_KEYPAIR"
echo ""

# Step 4: Get wallet address
echo "[4/5] Getting wallet address..."
WALLET_ADDRESS=$(solana address)
echo "  Wallet Address: $WALLET_ADDRESS"
echo ""

# Step 5: Request airdrop
echo "[5/5] Requesting 2 SOL airdrop from devnet faucet..."
echo "  (This may take a moment...)"

# Try airdrop with retries (devnet faucet can be flaky)
MAX_RETRIES=3
RETRY_COUNT=0

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    if solana airdrop 2 2>/dev/null; then
        echo "  Airdrop successful!"
        break
    else
        RETRY_COUNT=$((RETRY_COUNT + 1))
        if [ $RETRY_COUNT -lt $MAX_RETRIES ]; then
            echo "  Airdrop failed. Retrying in 5 seconds... (attempt $RETRY_COUNT/$MAX_RETRIES)"
            sleep 5
        else
            echo ""
            echo "  WARNING: Airdrop failed after $MAX_RETRIES attempts."
            echo "  The devnet faucet may be rate-limited or down."
            echo ""
            echo "  Alternative: Use the web faucet:"
            echo "    https://faucet.solana.com/"
            echo "    Wallet: $WALLET_ADDRESS"
        fi
    fi
done
echo ""

# Show final balance
echo "========================================"
echo "  Setup Complete!"
echo "========================================"
echo ""
echo "  Wallet Address: $WALLET_ADDRESS"
echo "  Balance: $(solana balance)"
echo "  Network: Devnet ($DEVNET_RPC)"
echo "  Keypair: $DEVNET_KEYPAIR"
echo ""
echo "  To run Butters with devnet config:"
echo "    cargo run -- --config config/devnet.toml"
echo ""
echo "  To check balance anytime:"
echo "    solana balance"
echo ""
echo "  To request more SOL:"
echo "    solana airdrop 2"
echo ""
