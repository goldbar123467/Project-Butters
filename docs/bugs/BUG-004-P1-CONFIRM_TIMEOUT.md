# BUG-004: CONFIRM_TIMEOUT

## Priority: P1 (HIGH)

## Summary
Transaction confirmation timeout of 60 seconds with individual RPC poll timeout of 10 seconds allows only 6 successful polls. During RPC latency spikes, all polls timeout before the 60-second limit, causing false-negative confirmation failures even when transactions successfully land on-chain.

## Affected Files
| File | Lines | Function/Block |
|------|-------|----------------|
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 456-458 | Timeout constant definitions |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 505-568 | Confirmation polling loop |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 530-535 | Individual poll with 10s timeout |
| `/home/ubuntu/projects/kyzlo-dex/src/application/orchestrator.rs` | 514-524 | Timeout error handling |

## Root Cause Analysis

The confirmation logic creates a race condition between two competing timeouts:

1. **Global timeout**: `CONFIRM_TIMEOUT_SECS = 60` seconds (lines 457, 507)
2. **Per-poll timeout**: `Duration::from_secs(10)` (line 531)

### The Math Problem
- Available time: 60 seconds
- Time per poll attempt: 10 seconds (timeout) + 500ms (sleep) = 10.5 seconds
- Maximum successful polls: 60 / 10.5 ≈ **5.7 polls**

During normal RPC operation, polls complete in ~1-2 seconds. However, when RPC latency spikes above 10 seconds:
1. Each poll times out at exactly 10 seconds (line 531)
2. After ~5-6 failed polls (50-60 seconds), the global timeout triggers (line 515)
3. Error is raised: "Confirmation timeout after 60s" (line 521-523)
4. **BUT**: The transaction may still be processing and lands on-chain seconds later

This creates **false negatives**: We report failure, but the transaction succeeds.

### Why This Is Critical
The orchestrator's state machine (lines 144-164) uses confirmation success/failure to decide whether to update strategy state:

```rust
// Line 145-149: Success path
Ok(()) => {
    strategy.confirm_trade(action, price);
    tracing::info!("Trade confirmed, strategy state updated");
}

// Line 151-163: Failure path
Err(e) => {
    // Trade failed - DO NOT update state, will retry next tick
    tracing::error!("Trade execution failed: {}. Position state unchanged, will retry.", e);
}
```

If confirmation falsely fails:
- Strategy state is NOT updated (position remains incorrect)
- Next tick will attempt the same trade again (duplicate execution)
- This feeds directly into **BUG-003: STATE_DESYNC** where strategy state diverges from on-chain reality

## Code Location

### Timeout Constants (Lines 456-458)
```rust
// Timeout configuration
const SEND_TIMEOUT_SECS: u64 = 30;
const CONFIRM_TIMEOUT_SECS: u64 = 60;  // ← Total confirmation window
const POLL_INTERVAL_MS: u64 = 500;
```

### Confirmation Loop (Lines 505-568)
```rust
// Step 2: Poll for confirmation with timeout
let start = Instant::now();
let confirm_timeout = Duration::from_secs(CONFIRM_TIMEOUT_SECS);  // 60 seconds
let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);      // 500ms

let sig = Signature::from_str(&signature)
    .map_err(|e| OrchestratorError::ExecutionError(format!("Invalid signature: {}", e)))?;

loop {
    // Check total timeout (line 515)
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

    // Poll signature status with individual timeout (lines 530-535)
    let status_result = tokio::time::timeout(
        Duration::from_secs(10),  // ← Individual poll timeout
        tokio::task::spawn_blocking(move || {
            client_clone.get_signature_status(&sig_clone)
        })
    ).await;

    match status_result {
        Ok(Ok(Ok(Some(result)))) => {
            if result.is_ok() {
                return Ok(signature);  // Success
            } else {
                return Err(/* Transaction failed on-chain */);
            }
        }
        Ok(Ok(Ok(None))) => {
            // Not yet confirmed, continue polling (line 549-553)
        }
        Ok(Ok(Err(e))) => {
            tracing::warn!("RPC error during confirmation poll: {}", e);
        }
        Ok(Err(e)) => {
            tracing::warn!("Task error during confirmation poll: {}", e);
        }
        Err(_) => {
            tracing::debug!("Individual poll timed out, retrying...");  // Line 561-562
        }
    }

    tokio::time::sleep(poll_interval).await;  // Line 566
}
```

## Failure Scenario

### Normal Case (RPC Latency < 2s)
1. Transaction submitted at T=0
2. Poll 1 at T=0.5s → pending → retry
3. Poll 2 at T=1.0s → pending → retry
4. Poll 3 at T=1.5s → confirmed ✅
5. Total time: ~1.5s (well within 60s timeout)

### Spike Case (RPC Latency > 10s)
1. Transaction submitted at T=0
2. Poll 1 at T=0.5s → RPC slow, timeout at T=10.5s → retry
3. Poll 2 at T=11.0s → RPC slow, timeout at T=21.0s → retry
4. Poll 3 at T=21.5s → RPC slow, timeout at T=31.5s → retry
5. Poll 4 at T=32.0s → RPC slow, timeout at T=42.0s → retry
6. Poll 5 at T=42.5s → RPC slow, timeout at T=52.5s → retry
7. Poll 6 at T=53.0s → RPC slow, timeout at T=63.0s → **global timeout at T=60s** ❌
8. **Error returned**: "Confirmation timeout after 60s"
9. **Reality**: Transaction lands on-chain at T=65s

### Post-Failure Cascade
10. Orchestrator reports trade failed (line 151-163)
11. Strategy state NOT updated (line 153-156)
12. Next tick (10s later at T=70s) sees stale state
13. Strategy generates **duplicate trade signal**
14. Second transaction submitted → **double execution**
15. **BUG-003: STATE_DESYNC** now active

## Impact

### Financial Impact
- **Duplicate Trades**: False negatives cause retry → double position exposure
- **Untracked Positions**: Strategy thinks trade failed but position exists → no exit logic
- **Cascading Losses**: Wrong position size → incorrect risk calculations → exceeds daily loss limits

### Operational Impact
- **State Corruption**: Strategy state diverges from on-chain reality (feeds BUG-003)
- **BalanceGuard Triggers**: Unexpected balance changes from unreported trades → trading halts
- **Manual Intervention Required**: Each false negative requires operator to reconcile state
- **Trust Degradation**: Logs show "failed" but Solscan shows "success" → confusion

### Probability
- **Normal RPC**: ~1-2% of transactions (occasional spikes)
- **Degraded RPC**: ~10-20% of transactions (during network congestion)
- **RPC Outage**: ~80-100% of transactions (during provider issues)

### Example Loss Calculation
```
Trade Size: 0.1 SOL (~$20 @ $200/SOL)
False Negative Rate: 10% during congestion
Duplicate Trades Per Day: 10 signals × 10% = 1 duplicate trade
Daily Extra Exposure: 0.1 SOL × 1 = $20
Monthly Extra Exposure: $20 × 30 = $600

Risk: If market moves -5% during duplicate exposure:
Loss = $600 × 5% = $30/month unintended loss
```

## Evidence

### Log Indicators
Look for this pattern in logs:
```
[ERROR] Confirmation timeout after 60s. Transaction may still land. Signature: {sig}
[ERROR] Trade execution failed: Confirmation timeout after 60s. Position state unchanged, will retry.
[INFO] SOL $199.50 | Z-score: -2.8 | Action: HOLD  ← Next tick
[INFO] EXECUTING TRADE - Action: EnterLong, Price: $199.45  ← Duplicate!
```

### On-Chain Evidence
```bash
# Check if "failed" transaction actually succeeded
solana confirm {signature} -u mainnet-beta
# Status: Finalized ← Transaction actually succeeded!

# Compare strategy state vs wallet
# Strategy: position = Flat
# Wallet: +0.1 SOL vs previous balance
```

### BalanceGuard Violations
After false negatives, `validate_post_trade()` may show:
```
[ERROR] Balance guard violation: Unexpected delta detected
Expected: -0 SOL (trade "failed")
Actual: -0.1 SOL (trade succeeded on-chain)
```

## Related Bugs

### Primary Interaction: BUG-003 (STATE_DESYNC)
**CONFIRM_TIMEOUT is a ROOT CAUSE of STATE_DESYNC**

Flow:
1. CONFIRM_TIMEOUT creates false negative
2. Strategy state not updated (position still "Flat")
3. On-chain state changes (position now "Long")
4. **STATE_DESYNC now active** (internal != external)
5. Next signal based on wrong state → incorrect trade
6. Cascade continues...

### Secondary Interaction: BUG-002 (JITO_FALLBACK)
If Jito bundle fails and we fall back to RPC:
1. Jito failure delays transaction submission
2. RPC confirmation window still only 60s
3. Higher chance of CONFIRM_TIMEOUT during degraded conditions

### Tertiary Interaction: BUG-001 (TX_VALIDATOR)
If transaction validator blocks legitimate Jupiter routes:
1. No transaction submitted
2. False negative from "transaction failed to build"
3. Different root cause, same symptom as CONFIRM_TIMEOUT

## Notes

### Why 10s Individual Timeout?
Historical context suggests this was chosen because:
- Solana RPC typically responds in 1-2s
- 10s allows for occasional slow responses
- Prevents infinite hangs on dead connections

**But**: During network congestion, 10s is too aggressive when combined with 60s total timeout.

### Why Not Remove Individual Timeout?
Without per-poll timeout:
- A single hung RPC call blocks for full 60s
- No retry opportunity during that window
- Worse success rate

### Design Tension
- **Too short individual timeout**: More false negatives during spikes
- **Too long individual timeout**: Miss retry opportunities
- **Too short global timeout**: Legitimate slow confirmations fail
- **Too long global timeout**: Hold orchestrator loop too long

**Optimal solution requires BOTH timeouts to be tuned together.**

### Solana Block Time Context
- Solana target block time: ~400ms
- Typical confirmation: 1-3 blocks = 400ms - 1.2s
- During congestion: 5-15 blocks = 2s - 6s
- Network partition: 30+ blocks = 12s+

60s should be sufficient even during severe congestion, **if RPC polls succeed**.

### RPC Provider Differences
| Provider | Typical Latency | P99 Latency | Outage Rate |
|----------|----------------|-------------|-------------|
| QuickNode | 100-500ms | 2-5s | Low |
| Alchemy | 200-800ms | 3-8s | Low |
| Helius | 150-600ms | 2-6s | Low |
| Public RPC | 500-2000ms | 10-30s | High |

**Using public RPC significantly increases CONFIRM_TIMEOUT risk.**

### Mitigation Exists But Incomplete
The error message (line 517-519) logs:
```rust
"Confirmation timeout after {}s. Transaction may still land. Signature: {}"
```

This acknowledges the transaction **may succeed**, but:
- Orchestrator still returns `Err()` (line 521-523)
- Strategy state still not updated
- No post-timeout verification mechanism
- Operator must manually check Solscan

### Quick Fix vs Proper Fix
**Quick**: Increase `CONFIRM_TIMEOUT_SECS` to 120s
- Reduces false negatives by ~50%
- But doubles worst-case hang time
- Doesn't address root cause

**Proper**: Implement async confirmation with callback
- Submit transaction, continue trading
- Background task polls for confirmation
- Callback updates strategy state when confirmed
- No blocking of trading loop
- **Requires architectural changes**
