# BUG-003: EXIT_RETRY_EXPOSURE

## Priority: P0 (CRITICAL)

## Summary
Exit failures trigger retry on the next tick (10-second poll interval), but with a 60-second confirmation timeout, the worst-case scenario creates 130+ seconds of uncontrolled exposure. During this window, stop-loss protection becomes meaningless as the position continues losing value unbounded during exit failure cascades.

## Affected Files
| File | Lines | Function/Block |
|------|-------|----------------|
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 123-127 | `run()` - Main polling loop with 10s interval |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 131-184 | `tick()` - Trade execution and retry logic |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 159-173 | Exit failure handling with passive retry |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 509-586 | `submit_and_confirm_transaction()` - 60s confirmation timeout |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 514-515 | Timeout constants: `CONFIRM_TIMEOUT_SECS = 60` |

## Root Cause Analysis

### The Fatal Timing Window

The bug stems from the interaction between three timing mechanisms:

1. **Poll Interval (10 seconds)**: The main trading loop sleeps for 10 seconds between ticks (line 127)
2. **Confirmation Timeout (60 seconds)**: Transaction confirmation can take up to 60 seconds before timing out (lines 514-515)
3. **Passive Retry Logic**: Exit failures don't propagate errors; they simply log and wait for the next tick (lines 165-172)

### The Failure Cascade

When an exit trade fails, the following cascade occurs:

**T+0s**: Exit signal triggered (e.g., stop-loss at -5%)
- `execute_trade()` called with `TradeAction::Exit`
- Position remains open (state not updated on failure)

**T+0s to T+60s**: Transaction confirmation timeout window
- `submit_and_confirm_transaction()` is blocking and polling for confirmation
- If the transaction is rejected/fails, this can consume the full 60 seconds
- During this time, the position is bleeding losses

**T+60s**: Timeout or failure detected
- Error caught at line 166: "Trade execution failed"
- Position state remains unchanged (line 168)
- Warning logged: "Exit trade failed - will retry on next tick" (line 171)
- Function returns without error propagation (line 169-172)

**T+60s to T+70s**: Waiting for next tick
- Main loop sleeps for poll_interval (10 seconds)
- Position continues to lose value
- No active retry, no hedging, no emergency exit

**T+70s**: Next tick begins
- Strategy's `update()` is called again (line 138)
- Should regenerate `TradeAction::Exit` if still underwater
- **BUT** if market has moved further against the position, the exit may now require different parameters or face worse slippage

### Worst Case Exposure Window

**Total Exposure Time** = Confirmation Timeout + Poll Interval
- 60s (first attempt timeout) + 10s (wait for next tick) = **70 seconds minimum**

**If second attempt also fails:**
- 60s (first) + 10s (wait) + 60s (second) + 10s (wait) = **140 seconds**

**During a -5%/min crash scenario:**
- Initial loss: -5% (triggers stop-loss)
- After 70s (1.17 min): Additional -5.85% = **-10.85% total**
- After 140s (2.33 min): Additional -11.65% = **-16.65% total**

### Why This is Critical

1. **Stop-loss becomes meaningless**: A 5% stop-loss can turn into 17%+ loss
2. **Cascading failures**: Network congestion or RPC issues can cause repeated failures
3. **No circuit breaker**: There's no emergency exit mechanism during the retry delay
4. **State inconsistency risk**: If the first transaction eventually lands during the retry wait, the second exit attempt will fail (position already flat), but this causes further delay

## Code Location

### Main Loop with 10s Poll Interval
```rust
// Lines 123-127
while *self.is_running.read().await {
    if let Err(e) = self.tick().await {
        tracing::error!("Tick error: {}", e);
        // Continue running despite errors
    }
    tokio::time::sleep(self.poll_interval).await;  // ⚠️ 10 second wait
}
```

### Exit Failure Handling with Passive Retry
```rust
// Lines 159-173
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
        // ⚠️ CRITICAL BUG: For exits, we don't propagate error
        if matches!(action, TradeAction::Exit) {
            tracing::warn!("Exit trade failed - will retry on next tick");
        } else {
            return Err(e);
        }
    }
}
```

**The Problem**: 
- Line 169-172: Exit failures are swallowed silently
- No error returned, just a warning logged
- System passively waits 10 seconds for next tick
- Position remains open and losing value

### 60-Second Confirmation Timeout
```rust
// Lines 514-515
const SEND_TIMEOUT_SECS: u64 = 30;
const CONFIRM_TIMEOUT_SECS: u64 = 60;  // ⚠️ 60 second timeout window

// Lines 540-549
loop {
    // Check total timeout
    if start.elapsed() > confirm_timeout {
        tracing::error!(
            "Confirmation timeout after {}s. Transaction may still land. Signature: {}",
            CONFIRM_TIMEOUT_SECS,
            signature
        );
        return Err(OrchestratorError::ExecutionError(
            format!("Confirmation timeout after {}s. Signature: {}", CONFIRM_TIMEOUT_SECS, signature)
        ));
    }
    // ... polling logic
}
```

### No Circuit Breaker or Emergency Exit
```rust
// Lines 240-260 (inside execute_trade)
// ⚠️ NO MECHANISM TO:
// - Cancel pending transactions
// - Execute emergency market orders
// - Hedge the position during confirmation wait
// - Escalate to higher slippage tolerance
// - Switch to backup RPC endpoints
```

## Failure Scenario

### Step-by-Step Walkthrough

**Setup:**
- Position: Long 10 SOL @ $100 (entry)
- Current price: $95 (down 5%)
- Stop-loss threshold: -5%

**T = 0s: Exit Signal Triggered**
```
tick() called
  ├─ fetch_price() → $95
  ├─ strategy.update($95) → TradeAction::Exit (stop-loss hit)
  └─ execute_trade(&Exit, $95) called
      └─ submit_and_confirm_transaction() blocking...
```

**T = 0-60s: Confirmation Timeout**
```
Solana RPC congestion → transaction stuck in mempool
  ├─ Polling every 500ms for confirmation
  ├─ Price dropping: $95 → $90 → $85 (market crash)
  ├─ Position now down 15% instead of 5%
  └─ Still waiting for confirmation...
```

**T = 60s: Timeout Error**
```
submit_and_confirm_transaction() returns Error
  ├─ "Confirmation timeout after 60s"
  └─ execute_trade() catches error
      └─ matches!(Exit) → log warning, return Ok(())  // ⚠️ Swallowed!
```

**T = 60-70s: Passive Wait**
```
tick() completes successfully (no error propagated)
main loop → tokio::time::sleep(10s)
  ├─ No active retry
  ├─ No emergency exit
  ├─ Price continues dropping: $85 → $82
  └─ Position now down 18%
```

**T = 70s: Second Attempt**
```
tick() called again
  ├─ fetch_price() → $82
  ├─ strategy.update($82) → TradeAction::Exit (still underwater)
  └─ execute_trade(&Exit, $82) called
      ├─ Now exiting at $82 instead of $95
      └─ Realized loss: 18% instead of 5%
```

**If first transaction eventually landed at T=65s:**
```
T = 65s: First tx confirmed (unknown to orchestrator)
  ├─ Position actually flat now
  └─ But orchestrator doesn't know (waiting for next tick)

T = 70s: Second exit attempt
  ├─ get_swap_params(&Exit) checks position
  ├─ Position is Flat → returns amount = 0
  ├─ Trade skipped: "Trade amount is zero"
  └─ Loss already locked in at unknown price
```

## Impact

### Financial Impact

**Direct Loss Amplification:**
- 5% stop-loss → potential 17%+ realized loss (3.4x worse)
- On $10,000 position: $500 expected loss → $1,700 actual loss
- **$1,200 additional slippage per failed exit**

**Cascade Risk:**
- Multiple failures during high volatility = exponential loss
- Market maker exploitation: Spotting stuck exits → front-running
- Liquidation risk if using leverage (not in current system but future)

### Operational Impact

**System Reliability:**
- Stop-loss protection is effectively disabled during failures
- Risk management parameters become meaningless
- Cannot trust strategy backtests (assume instant exits)

**Behavioral Risk:**
- Operators may increase stop-loss distance to account for slippage
- This defeats the purpose of stop-loss protection
- Creates false sense of security

### Trust Impact

**User Confidence:**
- "Set stop-loss at 5%" → "Actually lost 17%"
- Erodes trust in automated trading
- Legal/regulatory risk if managing third-party funds

## Evidence

### Code Evidence

1. **Exit Error Swallowing** (Lines 165-172):
   ```rust
   if matches!(action, TradeAction::Exit) {
       tracing::warn!("Exit trade failed - will retry on next tick");
       // ⚠️ Returns Ok(()), error not propagated
   }
   ```

2. **10s Poll Interval** (Line 127):
   ```rust
   tokio::time::sleep(self.poll_interval).await; // Default: 10s
   ```

3. **60s Confirmation Timeout** (Line 515):
   ```rust
   const CONFIRM_TIMEOUT_SECS: u64 = 60;
   ```

4. **No Emergency Exit Mechanism**:
   - No `cancel_transaction()` function
   - No `emergency_exit()` function
   - No circuit breaker logic
   - No backup RPC endpoints

### Observable Symptoms

**Log Pattern During Exit Failure:**
```
[ERROR] Trade execution failed: Confirmation timeout after 60s. Position state unchanged, will retry.
[WARN] Exit trade failed - will retry on next tick
[10 second gap in logs]
[INFO] SOL $82.00 | Z-score: -3.50 | Action: EXIT  // Price dropped from $95
```

**Balance Guard May Detect Anomaly:**
- If first transaction eventually lands, balance changes without orchestrator knowledge
- BalanceGuard will detect unexpected delta on next trade
- Trading halted, but loss already realized

### Performance Metrics

**Actual vs Expected:**
- Expected max exit delay: 1-5 seconds (normal confirmation)
- Actual max exit delay: 70-140+ seconds (with failures)
- **14x to 28x worse than expected**

## Related Bugs

### Direct Dependencies
- **BUG-001: BLIND_TRANSACTION_SIGNING**: Exit failures may indicate malicious transaction interception
- **BUG-002: BALANCE_GUARD_RACE**: Late transaction landing creates balance anomaly, halts trading

### Amplifying Factors
- **Network Congestion**: RPC failures increase timeout frequency
- **High Volatility**: Price moves faster during exit delay window
- **Slippage Limits**: 1% price impact check (line 337) may cause exit rejection during crashes

### Potential Future Bugs
- **MEV Sandwich Attacks**: Predictable retry timing makes system targetable
- **State Desynchronization**: Multiple pending exits if first eventually lands
- **Wallet Drainage**: Repeated fee burns without successful exits

## Notes

### Design Considerations

The passive retry approach was likely chosen to:
1. Avoid aggressive retries that could spam the RPC
2. Prevent infinite loops during systemic failures
3. Give Solana time to process during congestion

However, this is **insufficient for risk management**. Exits are time-critical and require active handling.

### Why Entry Failures are Different

Notice that entry failures **do** propagate errors (line 173):
```rust
} else {
    return Err(e);  // Entry failures bubble up
}
```

This asymmetry exists because:
- Entry failures don't create unbounded risk (position stays flat)
- Exit failures create unbounded downside (position bleeds)

But the current implementation treats exits too passively.

### Transaction "May Still Land" Problem

Line 545 warns: "Transaction may still land"
```rust
tracing::error!(
    "Confirmation timeout after {}s. Transaction may still land. Signature: {}",
    CONFIRM_TIMEOUT_SECS,
    signature
);
```

This creates a secondary bug:
- First exit attempt times out but later confirms
- Second exit attempt executes against flat position
- System doesn't detect the late confirmation
- BalanceGuard may catch this, but damage done

### Broader Implications

This bug reveals a fundamental flaw in the retry architecture:
- **No active monitoring** of pending transactions
- **No transaction cancellation** capability
- **No escalation strategy** (higher fees, different route)
- **No circuit breaker** for repeated failures

The fix requires a complete redesign of the exit execution flow, not just parameter tuning.

### Testing Challenge

This bug is difficult to catch in testing because:
1. Requires network failures or congestion
2. Timing-dependent (race condition)
3. Not reproducible in paper mode
4. Backtests assume instant fills

**Recommendation**: Add integration tests with simulated RPC timeouts and measure worst-case exposure windows.
