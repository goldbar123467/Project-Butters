#!/bin/bash
# Robust startup for Butters bot
cd /home/ubuntu/projects/kyzlo-dex

# Kill any existing
pkill -f "butters run" 2>/dev/null || true
tmux kill-session -t ironwatch 2>/dev/null || true
sleep 1

mkdir -p logs

# Run butters directly in tmux - simplest approach
tmux new-session -d -s ironwatch \
  "./target/release/butters run --live --i-accept-losses --config config.toml"

echo "✓ Butters started in tmux session 'ironwatch'"
echo ""
echo "  Attach:  tmux attach -t ironwatch"
echo "  Detach:  Ctrl+B, D"
