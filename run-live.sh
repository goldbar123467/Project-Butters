#!/bin/bash
# Butters Live Trading - One-shot launcher
# Usage: ./run-live.sh
set -euo pipefail

cd /home/ubuntu/projects/kyzlo-dex

echo "=== Butters Live Trading ==="
echo ""

# Check if systemd service is already running
if systemctl is-active --quiet butters 2>/dev/null; then
    echo "WARNING: Butters is already running as a systemd service!"
    echo ""
    echo "  To view logs:    journalctl -u butters -f"
    echo "  To stop service: sudo systemctl stop butters"
    echo ""
    echo "Running multiple instances may cause conflicts."
    read -p "Continue anyway? (y/N): " confirm
    if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
        echo "Aborted."
        exit 0
    fi
    echo ""
fi

# Check wallet balance
WALLET="6jtfgVtrL2NVSGXVJzZ6jkNV1726GpEXKu31KJzkdLkx"
BALANCE=$(/home/ubuntu/.local/share/solana/install/releases/2.1.21/solana-release/bin/solana balance "$WALLET" --url mainnet-beta 2>/dev/null | awk '{print $1}')

echo "Wallet: $WALLET"
echo "Balance: $BALANCE SOL"
echo ""

# Safety check
if (( $(echo "$BALANCE < 0.05" | bc -l) )); then
    echo "ERROR: Balance too low for live trading (need >= 0.05 SOL)"
    exit 1
fi

echo "Starting live trading with MEV protection (Jito)..."
echo "Press Ctrl+C to stop"
echo ""

./target/release/butters run --config config.toml --live --i-accept-losses
