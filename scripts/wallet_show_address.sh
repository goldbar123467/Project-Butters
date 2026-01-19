#!/bin/bash
# Prints wallet public address ONLY (no secrets)
# Usage: ./wallet_show_address.sh [keypair_path]
set -euo pipefail

SOLANA_BIN="${SOLANA_BIN:-/home/ubuntu/.local/share/solana/install/releases/2.1.21/solana-release/bin}"
KEYPAIR="${1:-/opt/solana/wallets/trading/id.json}"

if [[ ! -f "$KEYPAIR" ]]; then
    echo "ERROR: Keypair not found at $KEYPAIR" >&2
    exit 1
fi

"$SOLANA_BIN/solana-keygen" pubkey "$KEYPAIR"
