#!/bin/bash
# Prints wallet balance (no secrets)
# Usage: ./wallet_balance.sh [keypair_path]
set -euo pipefail

SOLANA_BIN="${SOLANA_BIN:-/home/ubuntu/.local/share/solana/install/releases/2.1.21/solana-release/bin}"
KEYPAIR="${1:-/opt/solana/wallets/trading/id.json}"
RPC_URL="${SOLANA_RPC_URL:-https://api.mainnet-beta.solana.com}"

if [[ ! -f "$KEYPAIR" ]]; then
    echo "ERROR: Keypair not found at $KEYPAIR" >&2
    exit 1
fi

ADDRESS=$("$SOLANA_BIN/solana-keygen" pubkey "$KEYPAIR")
"$SOLANA_BIN/solana" balance "$ADDRESS" --url "$RPC_URL"
