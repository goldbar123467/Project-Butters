#!/bin/bash
# Shows recent transaction signatures (no secrets)
# Usage: ./wallet_recent_txs.sh [keypair_path] [limit]
set -euo pipefail

SOLANA_BIN="${SOLANA_BIN:-/home/ubuntu/.local/share/solana/install/releases/2.1.21/solana-release/bin}"
KEYPAIR="${1:-/opt/solana/wallets/trading/id.json}"
RPC_URL="${SOLANA_RPC_URL:-https://api.mainnet-beta.solana.com}"
LIMIT="${2:-10}"

if [[ ! -f "$KEYPAIR" ]]; then
    echo "ERROR: Keypair not found at $KEYPAIR" >&2
    exit 1
fi

ADDRESS=$("$SOLANA_BIN/solana-keygen" pubkey "$KEYPAIR")
"$SOLANA_BIN/solana" transaction-history "$ADDRESS" --url "$RPC_URL" --limit "$LIMIT"
