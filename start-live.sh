#!/bin/bash
#
# Butters Live Trading - Start Script
# Run with: ./start-live.sh
#

set -e

cd /home/ubuntu/projects/kyzlo-dex

echo "════════════════════════════════════════════════════════════════════"
echo "                    BUTTERS LIVE TRADING"
echo "════════════════════════════════════════════════════════════════════"

# Check if binary exists
if [ ! -f "./target/release/butters" ]; then
    echo "[!] Binary not found. Building..."
    cargo build --release
fi

# Create logs directory
mkdir -p logs

# Load environment variables if .env exists
if [ -f ".env" ]; then
    echo "[+] Loading .env file..."
    export $(grep -v '^#' .env | xargs)
fi

# Check wallet exists
WALLET_PATH="/opt/solana/wallets/trading/id.json"
if [ ! -f "$WALLET_PATH" ]; then
    echo "[!] ERROR: Wallet not found at $WALLET_PATH"
    echo "    Create wallet: solana-keygen new -o $WALLET_PATH"
    exit 1
fi

echo "[+] Wallet: $WALLET_PATH"
echo "[+] Config: config.toml"
echo "[+] Trade size: 0.1 SOL per signal"
echo "[+] Jito MEV protection: ENABLED"
echo ""
echo "════════════════════════════════════════════════════════════════════"
echo "  SECURITY FEATURES ACTIVE:"
echo "  - Pre-sign TX validation (rejects unauthorized destinations)"
echo "  - Balance delta detection (halts on unexplained losses)"
echo "  - Jito tip accounts verified (8 official addresses)"
echo "════════════════════════════════════════════════════════════════════"
echo ""
echo "[!] Starting in 5 seconds... Press Ctrl+C to cancel"
sleep 5

echo ""
echo "[+] Starting Butters..."
echo ""

# Run with info logging, output to both terminal and log file
RUST_LOG=info ./target/release/butters run --config config.toml 2>&1 | tee -a logs/butters-$(date +%Y%m%d-%H%M%S).log
