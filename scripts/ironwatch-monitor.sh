#!/bin/bash
# IronWatch - Persistent Butters Trading Bot Monitor
# Usage: tmux new-session -d -s ironwatch './scripts/ironwatch-monitor.sh'

PROJECT_DIR="/home/ubuntu/projects/kyzlo-dex"
LOG_FILE="$PROJECT_DIR/logs/butters.log"
ALERT_LOG="$PROJECT_DIR/logs/ironwatch-alerts.log"
PID_FILE="$PROJECT_DIR/logs/butters.pid"
RESTART_DELAY=5
MAX_RESTARTS=5
RESTART_COUNT=0
RESTART_WINDOW=3600

mkdir -p "$PROJECT_DIR/logs"

log_alert() {
    echo "[$(date -Iseconds)] ALERT: $1" | tee -a "$ALERT_LOG"
}

log_info() {
    echo "[$(date -Iseconds)] INFO: $1" | tee -a "$ALERT_LOG"
}

start_bot() {
    cd "$PROJECT_DIR" || return 1
    
    # Kill existing if any
    if [ -f "$PID_FILE" ]; then
        OLD_PID=$(cat "$PID_FILE")
        kill "$OLD_PID" 2>/dev/null || true
        sleep 2
        kill -9 "$OLD_PID" 2>/dev/null || true
        rm -f "$PID_FILE"
    fi

    log_info "Starting butters..."
    ./target/release/butters run --live --i-accept-losses --config config.toml >> "$LOG_FILE" 2>&1 &
    echo $! > "$PID_FILE"
    log_info "Butters started with PID $(cat "$PID_FILE")"
}

check_bot() {
    if [ ! -f "$PID_FILE" ]; then
        return 1
    fi
    
    PID=$(cat "$PID_FILE")
    if ! kill -0 "$PID" 2>/dev/null; then
        return 1
    fi
    
    return 0
}

restart_bot() {
    if [ "$RESTART_COUNT" -ge "$MAX_RESTARTS" ]; then
        log_alert "Max restarts reached - manual intervention required"
        return 1
    fi
    
    RESTART_COUNT=$((RESTART_COUNT + 1))
    log_info "Restarting butters (attempt $RESTART_COUNT/$MAX_RESTARTS)..."
    sleep "$RESTART_DELAY"
    start_bot
}

# Main
log_info "========================================"
log_info "IronWatch monitor starting..."
log_info "========================================"

# Start bot immediately
start_bot

# Health check loop
while true; do
    sleep 30
    
    if ! check_bot; then
        log_alert "Bot not running - restarting"
        restart_bot || {
            log_alert "Failed to restart - waiting for manual intervention"
            while true; do sleep 60; done
        }
    fi
done
