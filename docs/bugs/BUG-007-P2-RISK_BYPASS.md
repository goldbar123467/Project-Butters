# BUG-007: RISK_BYPASS

## Priority: P2 (LOW - Enhancement)

## Summary
Risk limits (`max_daily_trades` and `max_daily_loss_pct`) are only checked on entry actions, not exit actions. This is **by design** - the system always allows exits even when daily limits are breached to prevent being stuck in losing positions. However, the current `daily_pnl` tracking uses percentage values instead of absolute SOL amounts, which could be enhanced for better loss tracking accuracy.

## Affected Files

| File | Lines | Function/Block |
|------|-------|----------------|
| `src/strategy/mean_reversion.rs` | 89-92 | `update()` - Risk limit check bypasses exits |
| `src/strategy/mean_reversion.rs` | 212-225 | `check_risk_limits()` - Validates `daily_trades` and `daily_pnl` |
| `src/strategy/mean_reversion.rs` | 182-197 | `on_trade_executed()` - Exit PnL tracking (percentage-based) |
| `src/strategy/mean_reversion.rs` | 58-61 | Field definitions for `daily_trades` and `daily_pnl` |
| `src/application/orchestrator.rs` | 133-165 | `tick()` - Orchestrator respects risk bypass for exits |

## Root Cause Analysis

The design intentionally separates risk checking from exit logic through a conditional in the `update()` method:

```rust
// Line 89-92 in mean_reversion.rs
// Check risk limits (but NOT for Exit - always allow exit attempts)
if !self.check_risk_limits() && matches!(self.position, PositionState::Flat) {
    return Some(TradeAction::Hold);
}
```

**Key Design Decision**: Risk limits are only enforced when `position == Flat`. This means:
- **Entry trades** (Long/Short when Flat) → Risk limits checked ✓
- **Exit trades** (Exit when Long/Short) → Risk limits bypassed ✓

The `daily_pnl` is tracked as a percentage rather than absolute SOL amounts:

```rust
// Lines 184-193 in mean_reversion.rs
let pnl = match self.position {
    PositionState::Long { entry_price } => {
        (price - entry_price) / entry_price * 100.0
    }
    PositionState::Short { entry_price } => {
        (entry_price - price) / entry_price * 100.0
    }
    PositionState::Flat => 0.0,
};
self.daily_pnl += pnl;  // Accumulates percentages, not SOL amounts
```

## Code Location

### Entry Risk Check (Lines 89-92)
```rust
// src/strategy/mean_reversion.rs
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

### Risk Limits Function (Lines 212-225)
```rust
// src/strategy/mean_reversion.rs
/// Check if risk limits allow trading
fn check_risk_limits(&self) -> bool {
    // Check daily trade limit
    if self.daily_trades >= self.config.risk.max_daily_trades {
        return false;
    }

    // Check daily loss limit
    if self.daily_pnl <= -self.config.risk.max_daily_loss_pct {
        return false;
    }

    true
}
```

### PnL Tracking on Exit (Lines 182-197)
```rust
// src/strategy/mean_reversion.rs
TradeAction::Exit => {
    // Calculate and track P&L
    let pnl = match self.position {
        PositionState::Long { entry_price } => {
            (price - entry_price) / entry_price * 100.0  // Percentage-based
        }
        PositionState::Short { entry_price } => {
            (entry_price - price) / entry_price * 100.0  // Percentage-based
        }
        PositionState::Flat => 0.0,
    };
    self.daily_pnl += pnl;  // Accumulates percentages
    self.position = PositionState::Flat;
    self.last_trade_time = Some(Instant::now());
    self.entry_time = None;
}
```

### Orchestrator Honors Exit Bypass (Lines 142-164)
```rust
// src/application/orchestrator.rs
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
    // ...
}
```

## Current Behavior

**Step-by-step flow when risk limits are breached:**

### Scenario 1: At Daily Trade Limit (10/10 trades used)
1. Position is Long or Short → `update()` called with new price
2. Exit condition triggered (take profit, stop loss, or z-score)
3. Risk check at line 90: `self.position != Flat`, so check is **skipped**
4. `evaluate_action()` returns `TradeAction::Exit`
5. Orchestrator executes exit trade
6. `confirm_trade()` updates `position = Flat`, no increment to `daily_trades`
7. **Result**: Exit successful despite being at trade limit ✓

### Scenario 2: At Daily Loss Limit (-3.0% daily_pnl)
1. Position is Long with unrealized loss
2. Stop loss condition triggers exit
3. Risk check skipped (position != Flat)
4. Exit executes successfully
5. PnL calculated: `(97 - 100) / 100 * 100 = -3.0%`
6. `daily_pnl += -3.0`, now `-6.0%`
7. **Result**: Exit successful, `daily_pnl` now deeper in loss ✓

### Scenario 3: Attempted Entry After Limit Breach
1. Position is Flat, `daily_trades = 10` (at limit)
2. New entry signal (Long or Short)
3. Risk check at line 90: `self.position == Flat`, check **runs**
4. `check_risk_limits()` returns `false`
5. `update()` returns `TradeAction::Hold`
6. **Result**: Entry blocked ✓

## Why This Is By Design

### 1. Safety First - Never Get Stuck in Positions
If exits were blocked by daily limits, a trader could be forced to hold a losing position overnight or longer when the daily loss limit is reached. This creates **unacceptable risk**:

- **Overnight Gap Risk**: Position could gap against you when markets reopen
- **Cascading Losses**: Unable to cut losses leads to deeper drawdowns
- **Margin Calls**: For leveraged positions, inability to exit could trigger liquidation
- **Missed Opportunities**: Capital trapped in bad trades can't be redeployed

### 2. Risk Management Hierarchy
The system implements a hierarchy of controls:

**Primary Controls (Always Active)**
- Per-trade stop loss (2.0%)
- Per-trade take profit (1.5%)
- Time-based stops
- Z-score mean reversion exits

**Secondary Controls (Entry Only)**
- Daily trade limit (10 trades)
- Daily loss limit (-3.0%)

This ensures the **primary controls** (which protect individual positions) always take precedence over **secondary controls** (which pace trading activity).

### 3. Real-World Trading Best Practice
Professional trading systems follow the principle: **"You can always exit, but you can't always enter."**

This mirrors how circuit breakers work in traditional markets:
- Trading halts block **new positions**
- Existing positions can still be **closed out**

### 4. Orchestrator Retry Logic
The orchestrator specifically retries failed exits but fails fast on entries (lines 158-162):

```rust
if matches!(action, TradeAction::Exit) {
    tracing::warn!("Exit trade failed - will retry on next tick");
} else {
    return Err(e);
}
```

This ensures exits are given maximum opportunity to succeed.

## Potential Enhancement

### Current Limitation: Percentage-Based PnL
The `daily_pnl` field accumulates **percentage returns** rather than **absolute SOL losses**:

```rust
self.daily_pnl += pnl;  // pnl is percentage (e.g., -2.5%)
```

**Problem**: This doesn't reflect actual capital at risk.

**Example**:
- Trade 1: Lose 2.5% on 0.1 SOL = -0.0025 SOL
- Trade 2: Lose 2.5% on 0.5 SOL = -0.0125 SOL
- `daily_pnl` = -5.0%, but actual loss = -0.015 SOL

The percentage-based tracking treats both losses equally, but the second trade lost 5x more capital.

### Proposed Enhancement
Track both percentage PnL **and** absolute SOL PnL:

```rust
pub struct MeanReversionStrategy {
    // ... existing fields ...

    /// Daily P&L percentage (existing)
    daily_pnl: f64,

    /// Daily P&L in absolute SOL (new)
    daily_pnl_sol: f64,

    /// Initial capital in SOL for percentage normalization (new)
    session_capital_sol: f64,
}
```

**Benefits**:
1. **Real Loss Tracking**: Know actual SOL gained/lost, not just percentages
2. **Better Risk Limits**: Set daily loss in SOL terms (e.g., "stop after -0.05 SOL lost")
3. **Capital Scaling**: Percentage losses mean different things at different capital levels
4. **Performance Metrics**: More accurate for reporting actual returns

**Implementation Approach**:
```rust
// On trade exit
TradeAction::Exit => {
    let pnl_pct = match self.position {
        PositionState::Long { entry_price } => {
            (price - entry_price) / entry_price * 100.0
        }
        PositionState::Short { entry_price } => {
            (entry_price - price) / entry_price * 100.0
        }
        PositionState::Flat => 0.0,
    };

    // Calculate absolute SOL PnL based on trade_size_sol
    let pnl_sol = (pnl_pct / 100.0) * self.config.trade_size_sol;

    self.daily_pnl += pnl_pct;      // Keep existing
    self.daily_pnl_sol += pnl_sol;  // Add new tracking

    // ... rest of exit logic ...
}
```

**Risk Limit Enhancement**:
```rust
fn check_risk_limits(&self) -> bool {
    // Check daily trade limit (existing)
    if self.daily_trades >= self.config.risk.max_daily_trades {
        return false;
    }

    // Check daily loss limit - percentage (existing)
    if self.daily_pnl <= -self.config.risk.max_daily_loss_pct {
        return false;
    }

    // NEW: Check daily loss limit - absolute SOL
    if let Some(max_sol_loss) = self.config.risk.max_daily_loss_sol {
        if self.daily_pnl_sol <= -max_sol_loss {
            return false;
        }
    }

    true
}
```

## Impact

**Current Risk**: LOW
- System works as designed
- Primary risk controls (stop loss, take profit) always active
- Exit bypass prevents capital being trapped in bad positions
- Percentage-based PnL tracking is functional but not optimal

**Enhancement Impact**: LOW-MEDIUM
- Better visibility into actual capital at risk
- More precise loss tracking for performance analysis
- Optional SOL-based loss limits for institutional use
- No changes to core exit bypass logic (which should remain)

**Implementation Priority**: P2 (Enhancement, not urgent)
- Current system is safe and operational
- Enhancement provides better observability
- Can be implemented incrementally without breaking changes

## Related Bugs

- **BUG-001-P1-JITO_SILENT_FAIL**: Exit retry logic depends on orchestrator (lines 158-162 of `orchestrator.rs`). If Jito fails and falls back to RPC silently, exits might not retry properly.
- **BUG-003-P1-STATE_MACHINE**: Exit state confirmation depends on `confirm_trade()` being called after on-chain success. If this is skipped, `daily_pnl` won't update.
- **BUG-005-P2-COOLDOWN**: Cooldown check also bypasses exits (line 85), similar pattern to risk limits bypass.

## Notes

### Design Philosophy Confirmed
After reviewing the code:
1. The exit bypass is **intentional and correct**
2. The orchestrator's retry-on-exit logic reinforces this design
3. The two-phase commit pattern (`update()` + `confirm_trade()`) prevents premature state changes
4. This matches professional trading system standards

### Enhancement vs Fix
This is classified as an **enhancement** rather than a **bug fix** because:
- Current behavior is correct and safe
- No capital is at risk due to this design
- The percentage-based tracking is accurate, just not as detailed as it could be
- Enhancement is about improving observability, not fixing broken logic

### Implementation Recommendation
If implementing the SOL-based tracking enhancement:
1. Add new fields alongside existing ones (don't replace)
2. Make SOL-based limits **optional** in config (for backward compatibility)
3. Default to percentage-based limits if SOL limits not configured
4. Add tests for both tracking methods
5. Update daily reset logic to clear both PnL fields
6. Add logging to show both percentage and SOL PnL on exits

### Code Review Checklist for Future Changes
When modifying risk limit logic:
- [ ] Exits must **always** bypass daily limits
- [ ] Entry trades must **always** respect daily limits
- [ ] Both `daily_trades` and `daily_pnl` checks must allow exits
- [ ] Orchestrator retry logic for exits must remain
- [ ] `check_risk_limits()` only called when `position == Flat`
- [ ] Test both percentage and absolute tracking if enhancement implemented
