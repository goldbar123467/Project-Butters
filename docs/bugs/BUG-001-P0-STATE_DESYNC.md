# BUG-001: STATE_DESYNC

## Priority: P0 (CRITICAL)

## Summary
Bot's internal position state can diverge from actual on-chain state when trades execute successfully on-chain but confirmation times out. This causes the strategy to believe no position exists when a real position is open, potentially leading to duplicate trades, incorrect position sizing, and hedging against oneself.

## Affected Files

| File | Lines | Function/Block |
|------|-------|----------------|
| `src/application/orchestrator.rs` | 144-164 | `tick()` - Trade execution and state update flow |
| `src/application/orchestrator.rs` | 449-568 | `submit_and_confirm_transaction()` - Timeout-based confirmation |
| `src/application/orchestrator.rs` | 515-523 | Confirmation timeout error handling |
| `src/strategy/mean_reversion.rs` | 78-99 | `update()` - Action generation without state mutation |
| `src/strategy/mean_reversion.rs` | 101-105 | `confirm_trade()` - State update only on confirmation |
| `src/strategy/mean_reversion.rs` | 167-200 | `on_trade_executed()` - Position state transitions |

## Root Cause Analysis

### The Two-Phase State Update Pattern
The codebase uses a two-phase commit pattern for state updates:

1. **Phase 1 (Line 134-137)**: `strategy.update()` generates a trade action **WITHOUT** mutating internal state
2. **Phase 2 (Line 147-148)**: `strategy.confirm_trade()` updates state **ONLY** after successful execution

This pattern is sound in theory - it prevents state updates for failed trades. However, it has a critical flaw:

### The Timeout Window Vulnerability

Lines 515-523 in `submit_and_confirm_transaction()`:
```rust
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
```

**The Problem**: This error path returns to the orchestrator's `tick()` function (line 151-162), which catches the error and **deliberately does NOT call `confirm_trade()`**:

```rust
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
```

### Why This Causes State Desync

**Timeout ≠ Failure**. The error message explicitly states: "Transaction may still land."

**Scenario:**
1. Bot sends trade transaction to Solana at T=0
2. Transaction is accepted into mempool at T=5s
3. Bot polls for confirmation every 500ms (line 566)
4. Network congestion / RPC lag prevents confirmation polling from returning success
5. Timeout fires at T=60s (line 515)
6. Error returned, `confirm_trade()` never called
7. Bot's state remains `PositionState::Flat`
8. **BUT**: Transaction lands on-chain at T=75s - real position is now open
9. Next tick: Bot thinks position is flat, generates duplicate entry signal
10. Result: Two positions open, bot thinks it has zero

### The Asymmetry Problem

The code treats timeouts as failures, but Solana treats them as pending:
- **Bot logic**: Timeout → failure → no state update → will retry
- **Solana reality**: Timeout → transaction still in mempool → may execute → position opens

This asymmetry is the root cause of all state desync bugs.

## Code Location

### Critical Path: Transaction Submission (Lines 449-568)

**Timeout Configuration:**
```rust
const SEND_TIMEOUT_SECS: u64 = 30;      // Line 456
const CONFIRM_TIMEOUT_SECS: u64 = 60;   // Line 457
const POLL_INTERVAL_MS: u64 = 500;      // Line 458
```

**Send Phase (Lines 467-501):**
- Sends transaction to RPC
- Times out after 30 seconds
- Returns signature if successful
- **Note**: Timeout here likely prevents desync (tx never entered mempool)

**Confirm Phase (Lines 505-567) - THE DANGER ZONE:**
```rust
loop {
    // Check total timeout
    if start.elapsed() > confirm_timeout {
        // ❌ BUG: Assumes timeout = failure
        return Err(OrchestratorError::ExecutionError(
            format!("Confirmation timeout after {}s. Signature: {}", CONFIRM_TIMEOUT_SECS, signature)
        ));
    }

    // Poll signature status
    match status_result {
        Ok(Ok(Ok(Some(result)))) => {
            if result.is_ok() {
                return Ok(signature);  // ✅ Only path that triggers state update
            }
        }
        // ... other error cases continue polling ...
    }

    tokio::time::sleep(poll_interval).await;
}
```

**State Update Gating (Lines 144-164):**
```rust
match self.execute_trade(&action, price).await {
    Ok(()) => {
        // ✅ Trade confirmed - update state
        let mut strategy = self.strategy.write().await;
        strategy.confirm_trade(action, price);
        tracing::info!("Trade confirmed, strategy state updated");
    }
    Err(e) => {
        // ❌ Trade "failed" (or timed out) - NO state update
        tracing::error!(
            "Trade execution failed: {}. Position state unchanged, will retry.",
            e
        );
    }
}
```

### Strategy State Machine (mean_reversion.rs)

**Action Generation (Lines 78-99):**
```rust
pub fn update(&mut self, price: f64) -> Option<TradeAction> {
    let zscore_result = self.zscore_gate.update(price)?;

    // ... risk checks ...

    let action = self.evaluate_action(&zscore_result, price);

    // NOTE: State is NOT updated here
    Some(action)
}
```

**State Update (Lines 101-105):**
```rust
pub fn confirm_trade(&mut self, action: TradeAction, price: f64) {
    self.on_trade_executed(action, price);  // Only path that changes position state
}
```

**Position State Transitions (Lines 167-200):**
```rust
fn on_trade_executed(&mut self, action: TradeAction, price: f64) {
    match action {
        TradeAction::EnterLong => {
            self.position = PositionState::Long { entry_price: price };
            // ...
        }
        TradeAction::EnterShort => {
            self.position = PositionState::Short { entry_price: price };
            // ...
        }
        TradeAction::Exit => {
            self.position = PositionState::Flat;  // Clear position
            // ...
        }
        TradeAction::Hold => {}
    }
}
```

## Failure Scenario

### Step-by-Step Walkthrough

**Initial State:**
- Bot position: `PositionState::Flat`
- Actual on-chain: No positions
- Z-score: -2.8 (oversold, below threshold of -2.5)

**T=0s: Trade Triggered**
```
orchestrator::tick() calls strategy.update(price=95.0)
→ ZScore: -2.8 (oversold)
→ Action: TradeAction::EnterLong
→ State: Still Flat (update() doesn't mutate state)
```

**T=1s: Transaction Built**
```
execute_trade() calls Jupiter API
→ Quote received: 0.1 SOL → USDC
→ Swap transaction built
→ Transaction signed with wallet
```

**T=2s: Transaction Submitted**
```
submit_and_confirm_transaction() sends tx to RPC
→ RPC accepts transaction
→ Returns signature: "5xK7...9pQz"
→ Enters confirmation polling loop
```

**T=2s-59s: Polling for Confirmation**
```
Every 500ms:
  client.get_signature_status(&sig)
  → Returns None (not yet confirmed)
  → Log: "Waiting for confirmation... (5.0s elapsed)"
  → Continue polling
```

**T=60s: TIMEOUT TRIGGERED**
```
start.elapsed() > confirm_timeout
→ Error logged: "Confirmation timeout after 60s. Transaction may still land."
→ Returns Err(OrchestratorError::ExecutionError(...))
→ orchestrator::tick() catches error
→ confirm_trade() NEVER CALLED
→ Bot state: Still PositionState::Flat
```

**T=75s: Transaction Actually Lands On-Chain**
```
Solana validator processes transaction from mempool
→ 0.1 SOL swapped to ~9.50 USDC
→ On-chain state: Position opened (long 0.1 SOL equivalent)
→ Bot state: Still thinks PositionState::Flat
→ STATE DESYNC ACHIEVED
```

**T=80s: Next Trading Tick**
```
orchestrator::tick() calls strategy.update(price=96.0)
→ Bot checks position: self.position = PositionState::Flat
→ Z-score: -2.6 (still oversold)
→ evaluate_action() sees Flat position
→ Action: TradeAction::EnterLong (again!)
→ Bot attempts SECOND long entry
```

**T=85s: Second Trade Executes**
```
Second EnterLong trade goes through
→ On-chain: Now long 0.2 SOL equivalent (double position)
→ Bot state: PositionState::Long { entry_price: 96.0 }
→ Bot thinks it has 1x position
→ Reality: It has 2x position
```

**T=150s: Price Reverts**
```
Price rises to 105.0
→ Z-score: +2.7 (overbought, above exit threshold of +2.5)
→ Bot: TradeAction::Exit
→ Executes ONE exit trade (closes 0.1 SOL)
→ Bot state: PositionState::Flat
→ On-chain reality: Still long 0.1 SOL (first position never closed)
```

**RESULT:**
- Bot believes it's flat with zero exposure
- Reality: Still long 0.1 SOL, exposed to price risk
- First position is now "invisible" to the bot
- Will continue trading, potentially opening more positions
- Invisible position accumulates P&L that bot doesn't track

## Impact

### Financial Risks

**1. Duplicate Position Accumulation**
- Each timeout → state desync → duplicate trade
- With 10 timeouts per day → 10x position than intended
- Example: Intend 0.1 SOL per trade → actually hold 1.0 SOL
- Risk multiplied by 10x without bot's awareness

**2. Untracked P&L**
- "Invisible" positions accumulate profit/loss
- Bot's daily_pnl tracking is incorrect
- Risk limits bypass: Bot may think it's at -3% daily loss when actually at -30%
- No stop-loss protection for invisible positions

**3. Self-Hedging**
- Bot may go long while having invisible short position
- Or short while having invisible long position
- Effectively trading against itself
- Paying fees and slippage to cancel out positions

**4. Position Size Violations**
- Config: max_position_pct = 5.0% (0.05 of portfolio)
- After 3 desyncs: Actual position = 15% of portfolio
- Violates risk management rules
- Potential for catastrophic losses on mean reversion failure

**5. Timeout Amplification**
- More positions → more exit attempts → more timeouts
- Exponential growth of invisible positions
- Could lead to liquidation-level exposure

### Operational Risks

**1. BalanceGuard False Alarms**
- BalanceGuard expects one trade's worth of SOL change
- Invisible positions cause unexpected balance deltas
- May trigger false circuit breaker halts
- Kills legitimate trading during profitable periods

**2. Lost Confidence in System**
- Operator sees balance decreasing with no trade logs
- Cannot reconcile bot's state with reality
- Manual intervention required to audit positions
- Destroys autonomous trading capability

**3. Cascading Failures**
- Timeout → desync → duplicate trade → another timeout → more desync
- Vicious cycle during network congestion
- RPC provider issues can trigger mass desync
- Bot becomes completely unreliable during high volatility (when it's needed most)

**4. Undetectable Until Damage Done**
- No log indicates "invisible position exists"
- Only symptom: Unexpected balance changes
- By the time detected, multiple positions may exist
- Requires manual on-chain analysis to discover

### Probability Assessment

**When Does This Happen?**
1. Network congestion (Solana mainnet during NFT mints, token launches)
2. RPC provider issues (rate limiting, degraded service)
3. Validator slowness (block production delays)
4. High priority fee competition (transaction lands late in block)

**Frequency Estimate:**
- Normal conditions: ~1-2% of trades timeout (but may still land)
- Network congestion: ~10-20% of trades timeout
- RPC provider issues: ~30-50% of trades timeout
- Trading 20 times per day → 0.4 desyncs/day (normal) to 10 desyncs/day (congestion)

**Expected Time to Disaster:**
- Conservative: 1 desync/week → 52 per year
- Each desync doubles exposure → Catastrophic by month 2-3
- Aggressive trading: Could hit disaster in days during volatility spike

## Evidence

### Log Signatures

**Timeout Error:**
```
ERROR Trade execution failed: Confirmation timeout after 60s. Signature: 5xK7...9pQz. Position state unchanged, will retry.
```

**Followed by unexpected behavior:**
```
INFO SOL $96.00 | Z-score: -2.60 | Action: HOLD
# (Should be HOLD because we just entered, but bot thinks we're flat)
```

**Later - Duplicate Entry:**
```
INFO EXECUTING TRADE - Action: EnterLong, Price: $96.50
# (Second entry when first position still open)
```

### On-Chain Evidence

**Signature Search:**
```bash
# Check transaction that timed out
solana confirm 5xK7...9pQz

# Expected: Transaction found, status: finalized
# Proves: Trade executed despite timeout
```

**Balance Audit:**
```bash
# Bot thinks: 0.5 SOL balance
# Actual query:
solana balance <wallet_pubkey>
# Returns: 0.3 SOL
# Missing 0.2 SOL → tied up in invisible positions
```

### BalanceGuard Symptoms

```
ERROR Balance guard violation: Expected delta: -100000000 lamports (SOL→Token), Actual delta: -300000000, Diff: -200000000 (200.00% of expected)
WARN Trading halted due to balance anomaly - manual review required
```

**Translation:**
- Expected one trade: 0.1 SOL spent
- Actually: 0.3 SOL spent
- Reason: Two invisible positions + one tracked position

### State Inconsistency Indicators

**Logged Position vs. Reality:**
```
# Bot logs:
INFO Position: Flat | Daily trades: 3 | Daily P&L: -0.5%

# On-chain query shows:
# - 2 open swap positions
# - Total exposure: 0.2 SOL
# - Actual P&L: -2.3%
```

## Related Bugs

### BUG-002: EXIT_RETRY_LOOP (Linked)
- When exit trades timeout, they're retried forever
- Each retry may create additional exit transactions
- Can close invisible positions without bot knowing
- Can also create "negative positions" (oversold)
- **Relationship**: Both caused by timeout ≠ failure assumption

### BUG-003: BALANCE_GUARD_RACE (Linked)
- BalanceGuard captures pre-trade balance
- If invisible position exists, pre-balance is already wrong
- Validation uses incorrect baseline
- May not catch further deviations
- **Relationship**: State desync defeats balance validation

### BUG-004: PNL_TRACKING_DRIFT (Consequence)
- `strategy.daily_pnl` only tracks confirmed trades
- Invisible positions' P&L not tracked
- Daily loss limit bypassed
- Risk limits ineffective
- **Relationship**: Direct consequence of state desync

### BUG-005: DUPLICATE_COOLDOWN_BYPASS (Consequence)
- Cooldown based on `last_trade_time`
- Not updated if confirm_trade() not called
- Allows rapid-fire duplicate trades
- Amplifies desync problem
- **Relationship**: Makes state desync worse

## Notes

### Design Intent vs. Reality

The two-phase commit pattern was clearly intentional:
```rust
// Line 97 comment in mean_reversion.rs:
// NOTE: State is NOT updated here - orchestrator must call confirm_trade() after success
```

**Intent**: Prevent state updates for genuinely failed trades (RPC errors, transaction rejected)

**Reality**: Cannot distinguish between:
1. Transaction rejected (failed) → correct to not update state
2. Transaction timeout (unknown) → incorrect to not update state
3. Transaction confirmed (success) → correct to update state

The code assumes binary states (success/failure) but Solana has three states (success/pending/failure).

### Why 60s Timeout?

Line 457: `const CONFIRM_TIMEOUT_SECS: u64 = 60;`

- Solana block time: ~400-500ms
- Expected confirmation: 2-10 seconds under normal conditions
- 60 seconds = very conservative
- **Yet still not enough** during congestion
- Increasing timeout delays error detection but doesn't fix root cause

### Alternative Approaches Considered

**1. Infinite Polling (No Timeout)**
- Pro: Would eventually see confirmation
- Con: Hangs bot indefinitely on genuine failures
- Con: Still doesn't solve the problem (RPC could be down)

**2. Query Position State from Chain**
- Pro: Ground truth from Solana
- Con: Requires tracking which tokens represent positions
- Con: Adds RPC call overhead to every tick
- Con: Doesn't prevent the duplicate trade (it already executed)

**3. Transaction Lifecycle Tracking**
- Pro: Could detect "pending" vs "failed"
- Con: Complex state machine (sent → pending → confirmed/failed/dropped)
- Con: Requires persistence across restarts
- Con: Dropped transactions still need timeout eventually

**4. Idempotency Keys / Nonces**
- Pro: Prevents duplicate execution
- Con: Not supported by Jupiter API
- Con: Would require custom transaction building
- Con: Adds significant complexity

### Current Mitigation

**From CLAUDE.md:**
```
### Before Changes
pkill -f "butters run" || true  # Stop bot first
```

**Manual Recovery:**
1. Stop bot
2. Query on-chain positions manually
3. Calculate actual exposure
4. Reset bot state to match reality
5. Restart bot

**This is NOT a solution** - it's a band-aid that requires constant operator vigilance.

### Recommended Fix Strategy

**Phase 1: Detection**
- Add position reconciliation query before each tick
- Compare bot state to on-chain reality
- Log discrepancies (don't halt, just warn)
- Gather data on desync frequency

**Phase 2: Recovery**
- Auto-correct bot state on desync detection
- Treat on-chain state as ground truth
- Update strategy.position to match reality
- Log reconciliation actions

**Phase 3: Prevention**
- Extend timeout to 120s (reduce false timeouts)
- Add "pending" state to position tracking
- Mark trades as "pending" after send, "confirmed" after confirmation
- Block new entries while pending trades exist
- Retry confirmation polling with exponential backoff

**Phase 4: Idempotency**
- Add transaction tracking database
- Store: signature, action, timestamp, status
- Check database before allowing duplicate action
- Prevent EnterLong when prior EnterLong is pending
- Allow Exit retries (idempotent action)

**Phase 5: Alternative Execution**
- Consider Jito bundles (atomic execution)
- Use transaction expiration properly
- Add explicit transaction drop detection
- Implement proper state machine for pending transactions

---

**CRITICAL SECURITY NOTE:**
This bug has **DIRECT FINANCIAL IMPACT**. It's not a "nice to have" fix - it's existential.
Every hour this bot runs with this bug is risking real SOL.
The probability of disaster increases with time and trading frequency.
**Priority must remain P0 until fully resolved.**
