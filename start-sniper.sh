#!/bin/bash
# Start Butters Sniper Bot - Extreme Z-Score Mode
# Only trades on 3+ consecutive bars with |z| >= 3.5

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Load environment
if [ -f .env ]; then
    export $(grep -v '^#' .env | xargs)
fi

# Create logs directory
mkdir -p logs

# Check if already running
if pgrep -f "butters-sniper run" > /dev/null; then
    echo "Sniper bot is already running!"
    exit 1
fi

# Start with timestamp in log filename
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOG_FILE="logs/sniper-${TIMESTAMP}.log"

echo "Starting Butters Sniper Bot..."
echo "  Config: config.toml"
echo "  Z-threshold: 3.5 (extreme only)"
echo "  Consecutive bars: 3"
echo "  Cooldown: none"
echo "  Log: $LOG_FILE"
echo ""

# Run the sniper bot
./target/release/butters-sniper run --config config.toml 2>&1 | tee "$LOG_FILE"
