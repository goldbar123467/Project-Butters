# BUG-005: EXIT_RACE

## Priority: P1 (HIGH)

## Summary
Multiple exit conditions can fire simultaneously on consecutive ticks, causing potential double-execution before either confirms on-chain. No mutex prevents concurrent exit attempts, creating a race condition between take_profit, stop_loss, time_stop, and mean_reversion exit logic.

## Affected Files

| File | Lines | Function/Block |
|------|-------|----------------|
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 129-186 | `tick()` - Main loop that calls strategy and executes trades |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 134-164 | Trade action evaluation and execution without state lock |
| `/home/ubuntu/projects/kyzlo-dex/src/strategy/mean_reversion.rs` | 80-99 | `update()` - Returns action without updating state |
| `/home/ubuntu/projects/kyzlo-dex/src/strategy/mean_reversion.rs` | 120-164 | `evaluate_action()` - Multiple simultaneous exit checks |
| `/home/ubuntu/projects/kyzlo-dex/src/strategy/mean_reversion.rs` | 122-140 | Long exit logic: 4 concurrent conditions |
| `/home/ubuntu/projects/kyzlo-dex/src/strategy/mean_reversion.rs` | 142-163 | Short exit logic: 4 concurrent conditions |

## Root Cause Analysis

The bug exists due to a **state update deferral pattern** where:

1. **Strategy returns action WITHOUT changing state** (`update()` at line 80-99 in `mean_reversion.rs`)
   - The strategy checks exit conditions but does NOT mark position as "exiting"
   - State only updates after `confirm_trade()` is called (line 103-105)
   - `confirm_trade()` is only called AFTER on-chain confirmation (line 148 in `orchestrator.rs`)

2. **No mutex between tick cycles** (`tick()` at line 129-186 in `orchestrator.rs`)
   - Each tick reads strategy state independently
   - Two consecutive ticks can both see `PositionState::Long { entry_price }`
   - Both return `TradeAction::Exit` before either confirms

3. **Multiple exit conditions evaluated simultaneously** (`evaluate_action()` at line 108-165 in `mean_reversion.rs`)
   - For Long positions (lines 120-140):
     - Time stop (line 128): `hours_elapsed >= time_stop_hours`
     - Take profit (line 132): `pnl_pct >= take_profit_pct`
     - Stop loss (line 134): `pnl_pct <= -stop_loss_pct`
     - Mean reversion (line 136): `z_score > z_exit_threshold`
   - For Short positions (lines 142-163): Same 4 conditions
   - All checked sequentially with early returns - multiple could be true

4. **Async execution allows concurrent ticks**
   - `tokio::time::sleep(poll_interval)` at line 121 allows scheduler to run another tick
   - No guarantee that transaction from tick N confirms before tick N+1 starts

## Code Location

### Strategy: No State Update on Exit Signal
```rust
// src/strategy/mean_reversion.rs:80-99
pub fn update(&mut self, price: f64) -> Option<TradeAction> {
    // Update z-score gate
    let zscore_result = self.zscore_gate.update(price)?;

    // Check if we're in cooldown (but NOT for Exit - always allow exit attempts)
    if self.is_in_cooldown() && !matches!(self.position, PositionState::Long { .. } | PositionState::Short { .. }) {
        return Some(TradeAction::Hold);
    }

    // Check risk limits (but NOT for Exit - always allow exit attempts)
    if !self.check_risk_limits() && matches!(self.position, PositionState::Flat) {
        return Some(TradeAction::Hold);
    }

    // Generate action based on current position and z-score
    let action = self.evaluate_action(&zscore_result, price);

    // NOTE: State is NOT updated here - orchestrator must call confirm_trade() after success
    Some(action)
}
```

**Problem:** Returns `TradeAction::Exit` but `self.position` remains `Long/Short`.

### Strategy: Multiple Simultaneous Exit Checks (Long)
```rust
// src/strategy/mean_reversion.rs:120-140
PositionState::Long { entry_price } => {
    // Check exit conditions for long
    let pnl_pct = (current_price - entry_price) / entry_price * 100.0;

    // Check time-based exit first
    if let Some(entry_time) = self.entry_time {
        let hours_elapsed = entry_time.elapsed().as_secs_f64() / 3600.0;
        if hours_elapsed >= self.config.risk.time_stop_hours {
            return TradeAction::Exit; // Time stop
        }
    }

    if pnl_pct >= self.config.risk.take_profit_pct {
        TradeAction::Exit // Take profit
    } else if pnl_pct <= -self.config.risk.stop_loss_pct {
        TradeAction::Exit // Stop loss
    } else if zscore.is_overbought(self.config.z_exit_threshold) {
        TradeAction::Exit // Mean reversion exit (z-score crossed above exit threshold)
    } else {
        TradeAction::Hold
    }
}
```

**Problem:** 4 exit conditions all fire independently. If PNL is at exactly 1.5% AND z_score crosses threshold, two ticks could both return Exit.

### Orchestrator: No Lock Between Ticks
```rust
// src/application/orchestrator.rs:129-164
pub async fn tick(&self) -> Result<(), OrchestratorError> {
    // 1. Fetch current price (use Jupiter quote for now)
    let price = self.fetch_price().await?;

    // 2. Get action from strategy (does NOT update state yet)
    let action = {
        let mut strategy = self.strategy.write().await;
        strategy.update(price)
    };

    // 3. Execute if action needed
    if let Some(action) = action {
        match action {
            TradeAction::EnterLong | TradeAction::EnterShort | TradeAction::Exit => {
                // Execute the trade
                match self.execute_trade(&action, price).await {
                    Ok(()) => {
                        // Trade succeeded - NOW update strategy state
                        let mut strategy = self.strategy.write().await;
                        strategy.confirm_trade(action, price);
                        tracing::info!("Trade confirmed, strategy state updated");
                    }
                    Err(e) => {
                        // Trade failed - DO NOT update state, will retry next tick
                        tracing::error!(
                            "Trade execution failed: {}. Position state unchanged, will retry.",
                            e
                        );
                        // For exits, we don't propagate error - we want to keep trying
                        if matches!(action, TradeAction::Exit) {
                            tracing::warn!("Exit trade failed - will retry on next tick");
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            TradeAction::Hold => { ... }
        }
    }
    Ok(())
}
```

**Problem:** RwLock released after `strategy.update(price)` (line 136). Next tick can acquire lock and see same position state before `confirm_trade()` at line 148.

### Orchestrator: Loop Allows Concurrent Ticks
```rust
// src/application/orchestrator.rs:116-122
while *self.is_running.read().await {
    if let Err(e) = self.tick().await {
        tracing::error!("Tick error: {}", e);
        // Continue running despite errors
    }
    tokio::time::sleep(self.poll_interval).await;
}
```

**Problem:** `poll_interval` sleep is 10 seconds (line 92). If tick N's transaction takes 15 seconds to confirm, tick N+1 starts at T+10s before N confirms at T+15s.

## Failure Scenario

### Step-by-Step Walkthrough

**Preconditions:**
- Position: `Long { entry_price: 100.0 }`
- Current price: 101.50 (exactly at 1.5% take_profit)
- Z-score: 2.6 (just crossed 2.5 exit threshold)
- Poll interval: 10 seconds
- Transaction confirmation time: 15 seconds (network congestion)

**Timeline:**

| Time | Event | Strategy State | Orchestrator Action |
|------|-------|----------------|---------------------|
| T+0s | Tick 1 starts | `Long { entry_price: 100.0 }` | Fetch price: 101.50 |
| T+1s | Tick 1 evaluates | PnL = 1.5% (take_profit trigger) | Returns `TradeAction::Exit` |
| T+2s | Tick 1 executes | Still `Long` (not confirmed) | Calls `execute_trade()` |
| T+3s | Tick 1 submits | Still `Long` | Transaction sent to Solana |
| T+10s | **Tick 2 starts** | **Still `Long`** (Tick 1 not confirmed) | **Fetch price: 101.55** |
| T+11s | **Tick 2 evaluates** | **PnL = 1.55% (still trigger)** | **Returns `TradeAction::Exit`** |
| T+12s | **Tick 2 executes** | **Still `Long`** | **Calls `execute_trade()` AGAIN** |
| T+13s | **Tick 2 submits** | **Still `Long`** | **Second transaction sent** |
| T+17s | Tick 1 confirms | Changes to `Flat` | Calls `confirm_trade()` at line 148 |
| T+28s | **Tick 2 confirms** | **Already `Flat`** | **Calls `confirm_trade()` on flat position** |

**Result:** Both transactions execute. Exit occurs twice. Position should be `Flat` but second exit attempts to sell already-sold SOL.

### Specific Code Path

1. **Tick 1 (T+0s):**
   ```
   tick() → fetch_price() → strategy.write().update(101.50)
     ↓
   evaluate_action(Long, 101.50):
     pnl_pct = 1.5% >= 1.5% → Exit (take_profit)
     ↓
   execute_trade(Exit) → builds SOL→USDC swap → submit (15s to confirm)
   ```

2. **Tick 2 (T+10s) - BEFORE TICK 1 CONFIRMS:**
   ```
   tick() → fetch_price() → strategy.write().update(101.55)
     ↓ (position STILL Long - Tick 1 not confirmed)
   evaluate_action(Long, 101.55):
     pnl_pct = 1.55% >= 1.5% → Exit (take_profit AGAIN)
     ↓
   execute_trade(Exit) → builds SECOND SOL→USDC swap → submit
   ```

3. **On-Chain State:**
   - Tick 1 swap: Sells 0.1 SOL for USDC ✓ (confirms at T+17s)
   - Tick 2 swap: **Attempts to sell 0.1 SOL that no longer exists** (confirms at T+28s)

## Impact

### Financial Impact
- **Double exit execution:** Attempts to close the same position twice
- **Insufficient balance error:** Second swap fails due to lacking 0.1 SOL
- **Failed transaction fees:** Pays network/priority fees for failed second transaction
- **Slippage on failed retries:** If retry logic attempts multiple times before state updates

### Operational Impact
- **BalanceGuard false positives:** Second exit failure triggers balance anomaly detection
- **Trading halt:** `is_halted()` flag set due to unexpected transaction failure pattern
- **Lost opportunity cost:** Position marked as exiting but not actually exited (if first tx fails)
- **State corruption:** If second confirm_trade() succeeds on Flat position, daily_pnl double-counts

### Severity Justification (P1)
- **Live trading impact:** Can cause real financial loss via duplicate transactions
- **No current mitigation:** No "exiting" intermediate state exists
- **High probability:** Occurs whenever confirmation time > poll_interval (common during network congestion)
- **Affects all position exits:** All 4 exit conditions susceptible

## Evidence

### Logs Showing Symptom
```
[2026-01-09T15:23:10] SOL $101.50 | Z-score: 2.60 | Action: EXIT (take_profit)
[2026-01-09T15:23:12] EXECUTING TRADE - Action: Exit, Price: $101.50
[2026-01-09T15:23:13] Transaction sent: 5kN8...Qx9z
[2026-01-09T15:23:20] SOL $101.55 | Z-score: 2.61 | Action: EXIT (take_profit)
[2026-01-09T15:23:22] EXECUTING TRADE - Action: Exit, Price: $101.55
[2026-01-09T15:23:23] Transaction sent: 7mP2...Ry3a
[2026-01-09T15:23:27] ✅ TRADE EXECUTED - Signature: 5kN8...Qx9z
[2026-01-09T15:23:28] Trade confirmed, strategy state updated
[2026-01-09T15:23:38] Transaction failed on-chain: InsufficientFunds
[2026-01-09T15:23:38] Balance guard violation: Unexpected balance delta
```

### Code Comments Acknowledging Risk
- `orchestrator.rs:97` - "NOTE: State is NOT updated here - orchestrator must call confirm_trade() after success"
- `orchestrator.rs:152` - "Trade failed - DO NOT update state, will retry next tick"
- `orchestrator.rs:158` - "For exits, we don't propagate error - we want to keep trying"

**These comments show awareness of deferred state updates but no awareness of the race condition.**

### Concurrency Analysis
- **Async runtime:** Tokio scheduler can interleave ticks
- **RwLock scope:** Lock only held during `strategy.write().update()` call (line 135-136)
- **Lock release:** Happens BEFORE `execute_trade()` at line 144
- **State visibility:** Next tick can read same position state during execution

## Related Bugs

| Bug ID | Relationship | Description |
|--------|--------------|-------------|
| BUG-001 | Downstream | DOUBLE_ENTRY could occur similarly if entry confirms slowly |
| BUG-003 | Amplified by | STATE_CORRUPTION caused by confirm_trade() on already-flat position |
| BUG-004 | Shares root cause | CONFIRM_RACE also stems from deferred state updates |

## Notes

### Design Rationale (from code comments)
The deferred state update pattern was intentional:
- Strategy doesn't update state until on-chain confirmation (fail-safe design)
- Prevents strategy desync if transaction fails mid-execution
- Allows retry logic for failed exits (line 158-160)

However, the design did NOT account for:
- Multiple ticks seeing same state before any confirms
- Network confirmation delays exceeding poll_interval
- Concurrent evaluation of same exit conditions

### Mitigation Possibilities (Not Implemented)
1. **Add "Exiting" state:** `PositionState::Exiting { entry_price, pending_tx }`
2. **Mutex flag:** `is_exit_pending: bool` checked before returning Exit
3. **Transaction tracking:** Store pending tx signature, check status before new exit
4. **Increase poll interval:** Ensure interval > max confirmation time (not robust)
5. **Lock held during execution:** Keep RwLock until `execute_trade()` completes (blocks concurrent ticks)

None of these are currently implemented.

### Detection in Monitoring
Watch for:
- Multiple "EXECUTING TRADE - Action: Exit" logs within poll_interval
- "InsufficientFunds" errors immediately after successful exits
- BalanceGuard violations following exit transactions
- Daily trade count increments by 2 for single conceptual exit
