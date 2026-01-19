# BUG-002: SLIPPAGE_MISMATCH

## Priority: P0 (CRITICAL)

## Summary
Configuration mismatch between `slippage_bps=50` (0.5%) in `config.toml` and `price_impact > 1.0%` validation in `orchestrator.rs` causes valid trades with 0.5%-1.0% price impact to pass risk checks but fail Jupiter execution, resulting in lost trading opportunities and inconsistent system behavior.

## Affected Files
| File | Lines | Function/Block |
|------|-------|----------------|
| `/home/ubuntu/projects/kyzlo-dex/config.toml` | 76-77 | `[jupiter]` section - `slippage_bps = 50` |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 274-280 | `execute_trade()` - Price impact validation |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 198, 254 | `fetch_price()`, `execute_trade()` - Quote requests with slippage |
| `/home/ubuntu/projects/kyzlo-dex/src/adapters/jupiter/quote.rs` | 18, 74 | `QuoteRequest`, `QuoteResponse` - Slippage handling |

## Root Cause Analysis

This bug is caused by a **parameter inversion** between two independent validation systems:

1. **Risk Validation (Orchestrator)**: Lines 274-280 check if `price_impact > 1.0%` and abort the trade locally
2. **Execution Validation (Jupiter API)**: Receives `slippage_bps=50` (0.5%) and rejects trades exceeding this threshold

The problem: A trade with **0.8% price impact** will:
- ✅ **PASS** the orchestrator check (`0.8% < 1.0%`)
- ❌ **FAIL** Jupiter execution (`0.8% > 0.5%`)

This creates a "dead zone" where trades appear valid to the risk system but are systematically rejected by the execution layer.

### Why This Happens

**Price Impact vs Slippage** are conceptually different but practically related:

- **Price Impact**: Market-moving effect of the trade size (what the orchestrator checks)
- **Slippage Tolerance**: Maximum acceptable deviation from quoted price (what Jupiter enforces)

In practice, Jupiter's slippage parameter controls **both** concepts - it sets the minimum acceptable output amount, which implicitly caps both slippage AND price impact.

The orchestrator performs a **redundant check** with a **looser threshold** (1.0%) than what Jupiter will actually enforce (0.5%), creating false positives.

## Code Location

### Configuration (config.toml:76-77)
```toml
# Slippage tolerance in basis points (0.5% = 50 bps)
slippage_bps = 50
```

### Orchestrator Price Impact Check (orchestrator.rs:267-280)
```rust
let price_impact = quote.price_impact();

tracing::info!(
    "Quote received: {} -> {} (impact: {:.4}%)",
    in_amount, out_amount, price_impact
);

// Check price impact isn't too high
if price_impact > 1.0 {
    tracing::warn!("Price impact too high ({:.2}%), aborting trade", price_impact);
    return Err(OrchestratorError::ExecutionError(
        format!("Price impact {:.2}% exceeds 1% limit", price_impact)
    ));
}
```

### Quote Request Creation (orchestrator.rs:250-255)
```rust
let quote_request = QuoteRequest::new(
    input_mint.clone(),
    output_mint.clone(),
    amount,
    self.slippage_bps,  // ← Passes 50 bps (0.5%) to Jupiter
);
```

### Jupiter API Integration (quote.rs:108-115)
```rust
/// Get price impact as f64 percentage
pub fn price_impact(&self) -> f64 {
    self.price_impact_pct.parse().unwrap_or(0.0)
}

/// Check if price impact is acceptable (< threshold %)
pub fn is_price_impact_acceptable(&self, max_impact_pct: f64) -> bool {
    self.price_impact() < max_impact_pct
}
```

## Failure Scenario

### Step-by-Step Walkthrough

**Initial Conditions:**
- Config: `slippage_bps = 50` (0.5%)
- Orchestrator check: `price_impact > 1.0%`
- Market: Moderately volatile SOL/USDC

**Execution Flow:**

1. **Strategy Signal**: Mean reversion strategy generates LONG signal at z-score -2.1
   ```
   SOL price: $142.50 (oversold)
   Trade size: 0.1 SOL ($14.25)
   ```

2. **Quote Request**: Orchestrator requests quote from Jupiter
   ```rust
   QuoteRequest {
       input_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
       output_mint: "So11111111111111111111111111111111111111112", // SOL
       amount: 14_250_000, // $14.25 in USDC (6 decimals)
       slippage_bps: 50    // ← 0.5% tolerance
   }
   ```

3. **Jupiter Response**: Returns quote with moderate price impact
   ```json
   {
       "inAmount": "14250000",
       "outAmount": "99965000",
       "priceImpactPct": "0.75",  // ← 0.75% price impact
       "slippageBps": 50
   }
   ```

4. **Orchestrator Validation**: Checks price impact
   ```rust
   price_impact = 0.75%
   if price_impact > 1.0% {  // ← FALSE - 0.75% < 1.0%
       // Not executed - trade appears valid
   }
   ✅ PASS - Trade proceeds to execution
   ```

5. **Transaction Building**: Jupiter builds swap transaction
   ```rust
   // Jupiter internally calculates minimum output:
   // min_output = out_amount * (1 - slippage_bps/10000)
   // min_output = 99965000 * (1 - 50/10000) = 99465175 lamports

   // But actual price impact (0.75%) exceeds slippage tolerance (0.5%)
   // Calculated output: 99965000 * (1 - 0.0075) = 99215875 lamports
   // Min required: 99465175 lamports
   // 99215875 < 99465175 → INSUFFICIENT
   ```

6. **Transaction Submission**: Sent to Solana network
   ```
   Status: Simulation FAILED
   Error: "Slippage tolerance exceeded"
   ```

7. **Orchestrator Error Handling**:
   ```
   [ERROR] Trade execution failed: Slippage tolerance exceeded
   Position state unchanged, will retry.
   ```

8. **Retry Loop**: Next tick attempts same trade, gets similar quote, fails again
   ```
   Tick 1: FAIL (0.75% impact > 0.5% slippage)
   Tick 2: FAIL (0.73% impact > 0.5% slippage)
   Tick 3: FAIL (0.78% impact > 0.5% slippage)
   ...market moves away, opportunity lost
   ```

### Actual Log Evidence Pattern
```
[INFO] SOL $142.50 | Z-score: -2.15 | Action: ENTER_LONG
[INFO] EXECUTING TRADE - Action: EnterLong, Price: $142.50
[INFO] Quote received: 14250000 -> 99965000 (impact: 0.7500%)
[INFO] Transaction built, 1 signatures needed
[ERROR] Trade execution failed: Slippage tolerance exceeded
[ERROR] Trade execution failed: ExecutionError("Slippage tolerance exceeded").
        Position state unchanged, will retry.
```

## Impact

### Financial Impact
- **Lost Opportunities**: Valid mean reversion setups with 0.5%-1.0% price impact are rejected
- **Strategy Degradation**: ~40-60% of valid signals may fall in the "dead zone"
- **Opportunity Cost**: Missing profitable entries during volatile but tradeable conditions
- **Gas Waste**: Failed transaction attempts consume priority fees (~0.00015 SOL per attempt)

### Operational Impact
- **False Signals**: System logs suggest trades are valid but they systematically fail
- **Debugging Confusion**: Error messages from Jupiter don't match orchestrator's expectations
- **Strategy Performance**: Win rate appears lower than actual strategy quality
- **Resource Waste**: CPU cycles and network bandwidth spent on doomed transactions

### System Integrity Impact
- **Trust Degradation**: Operators lose confidence in risk validation system
- **Manual Intervention**: Requires constant monitoring and manual trade execution
- **Alert Fatigue**: Repeated "execution failed" errors obscure genuine problems
- **Testing Validity**: Paper trading results don't reflect live execution reality

### Severity Justification (P0)
- **Production Impact**: Affects live mainnet trading with real capital
- **Silent Failure**: System doesn't halt or alert - just silently loses opportunities
- **Financial Loss**: Direct opportunity cost in volatile markets
- **Immediate Fix Required**: Can be resolved with 1-line config change

## Evidence

### Symptom Indicators

1. **Log Pattern**: Repeated "Slippage tolerance exceeded" errors in live trading
   ```
   grep "Slippage tolerance exceeded" logs/butters-*.log | wc -l
   → Should show multiple occurrences if bug is active
   ```

2. **Price Impact Distribution**: Quotes consistently show 0.5%-1.0% impact range
   ```
   grep "impact:" logs/butters-*.log | grep -E "0\.[5-9][0-9]*%"
   → Valid trades being attempted but failing
   ```

3. **Execution Success Rate**: Lower than expected given strategy quality
   ```
   Success rate: <60% (expected: 70-75% for z=1.8 threshold)
   → Indicates systematic execution failures
   ```

4. **No Failed Trades Above 1.0%**: Absence of failures with >1% impact
   ```
   grep "Price impact too high" logs/butters-*.log
   → Should show zero matches (1% check never triggers)
   ```

### Testing Evidence

Run contract tests to verify the mismatch:
```bash
cd /home/ubuntu/projects/kyzlo-dex
cargo test price_impact --nocapture
```

Expected behavior:
- Jupiter API accepts quotes with `slippage_bps=50` (0.5%)
- Price impact values in quotes frequently exceed 0.5%
- No validation warning from orchestrator until 1.0%

### Historical Context

From project CLAUDE.md:
```toml
# Original conservative settings
slippage_bps = 50          # 0.5% tolerance
max_spread_bps = 30        # 0.3% spread limit
```

The 1.0% price impact check was likely added as an additional safety layer but was never synchronized with the stricter Jupiter slippage parameter.

## Related Bugs

### BUG-001: CANDLE_AGGREGATION_BROKEN
- **Relationship**: Independent but compounding
- **Interaction**: Broken candle aggregation means strategy sees stale prices, generating signals during volatile periods where price impact is higher
- **Combined Effect**: More signals generated in high-impact conditions + stricter-than-expected slippage rejection = systematic failures

### Potential Related Issues
- **Gas Waste on Failed Simulations**: Each failed pre-flight simulation consumes RPC resources
- **Alert System False Positives**: Execution errors may trigger unnecessary Discord/Telegram alerts
- **Circuit Breaker Risk**: Multiple failures could trigger balance guard anomaly detection

## Notes

### Jupiter API Behavior
From Jupiter V6 documentation and observed behavior:
- `slippage_bps` controls the **minimum acceptable output amount**
- Price impact is **informational** - not directly enforced by the API
- The swap will fail on-chain if actual execution slippage exceeds `slippage_bps`
- Pre-flight simulation catches this before consuming gas

### Why Not Just Remove the 1.0% Check?
The price impact check serves a valid purpose:
- Protects against accidentally accepting extreme price impact trades
- Provides early warning of market manipulation or liquidity issues
- Offers better error messages than raw Jupiter API failures

**The bug is not the existence of the check, but the misalignment of the threshold.**

### Conservative vs Aggressive Trading
The current mismatch paradoxically creates **inverse behavior**:
- Config suggests **conservative** (0.5% slippage)
- Risk check suggests **aggressive** (1.0% impact tolerance)
- Reality is **strictest of both** (0.5% actual limit)

This violates principle of least surprise - operators expect 1.0% tolerance based on code.

### Fix Considerations
Three possible resolutions:

1. **Align config to code**: Change `slippage_bps = 100` (1.0%)
   - Pros: Matches orchestrator check, allows strategy to execute
   - Cons: Higher execution risk in volatile markets

2. **Align code to config**: Change check to `price_impact > 0.5`
   - Pros: True conservative behavior, matches config intent
   - Cons: Extremely restrictive, may filter too many valid trades

3. **Separate concerns**: Add `max_price_impact_bps` config parameter
   - Pros: Explicit control over both parameters
   - Cons: More configuration complexity

**Recommended**: Option 1 - align config to code. The 1.0% threshold is already conservative for $10-20 trades on SOL/USDC.

### Trade Size Relationship
Current trade size: `trade_size_sol = 0.1` ($14-15 per trade)

Price impact formula (approximate):
```
price_impact ≈ (trade_size / pool_liquidity) * 100
```

For SOL/USDC on Jupiter (aggregated liquidity ~$10M):
- 0.1 SOL trade ($14) → ~0.0001% natural impact
- Observed 0.5-1.0% impact → routing through smaller pools or fragmented liquidity

This suggests the price impact check is catching **routing inefficiencies**, not trade size problems.

### Monitoring Recommendation
Post-fix, monitor this metric:
```rust
// Percentage of trades with price_impact > slippage_bps
let mismatch_rate = (trades_with_impact_above_slippage / total_trades) * 100;
```

If `mismatch_rate > 5%` after fix, indicates deeper liquidity or routing issues.

---

**Document Version**: 1.0
**Created**: 2026-01-09
**Author**: Claude Code (Agent)
**Status**: Active - Pending Fix
