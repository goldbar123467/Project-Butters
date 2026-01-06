# Jupiter API Golden Response Fixtures

## Contract Version
Current version: `jupiter_v1`
Fixture naming pattern: `{endpoint}_{scenario}_v1.json`

## Schema Change Log

### 2026-01-06: feeAmount/feeMint removed from swapInfo
- **Fields removed**: `routePlan[].swapInfo.feeAmount`, `routePlan[].swapInfo.feeMint`
- **Reason**: Jupiter API no longer returns per-hop fee breakdown in quote responses
- **Impact**: These fields are now optional in contract tests
- **Detection**: Caught by live smoke test `test_live_quote_endpoint_schema`

## Purpose

These fixtures represent the **expected API contract** between our adapter and Jupiter's V6 API.
They are **immutable snapshots** that should only be updated when the external API contract
intentionally changes.

## Contract Testing Philosophy

These fixtures enable early breakage detection by:
1. Validating exact field presence (not just successful parsing)
2. Asserting correct types for all fields
3. Checking semantic invariants (amounts, fees, routing)
4. Detecting renamed or removed fields immediately

**Goal**: Turn upstream API changes into immediate, actionable test failures.

## Fixtures

### Quote Endpoint (`GET /quote`)

| File | Scenario | Purpose |
|------|----------|---------|
| `quote_sol_usdc_v1.json` | Simple SOL→USDC swap | Baseline single-hop route with platform fee |
| `quote_multi_hop_v1.json` | Multi-hop with split | Tests route plan with multiple AMMs and split routing |
| `quote_high_impact_v1.json` | High price impact | Tests risk checks and slippage thresholds |

### Swap Endpoint (`POST /swap`)

| File | Scenario | Purpose |
|------|----------|---------|
| `swap_v1.json` | Standard swap transaction | Baseline transaction building response |
| `swap_with_priority_v1.json` | High priority fee | Tests prioritization fee handling |

## Updating These Fixtures

**DO NOT** regenerate these files automatically. Updates require:

1. A clear understanding of **why** the contract changed
2. Explicit approval in code review
3. A commit message explaining the contract change, e.g.:
   ```
   fix(jupiter): Update golden fixtures for API v6.2

   Jupiter added 'newField' to quote response.
   See: https://docs.jup.ag/changelog/v6.2
   ```

## Field Criticality

Fields validated by contract tests are marked by criticality:

### Quote Response - Critical Fields
- `inAmount` - Input amount (used in execution)
- `outAmount` - Output amount (used in strategy decisions)
- `otherAmountThreshold` - Minimum output after slippage
- `slippageBps` - Slippage tolerance
- `priceImpactPct` - Price impact percentage
- `routePlan` - Routing information
- `routePlan[].swapInfo.ammKey` - Pool identifier
- `routePlan[].swapInfo.label` - DEX label
- `routePlan[].swapInfo.feeAmount` - Fee charged (OPTIONAL - removed by Jupiter as of 2026-01-06)
- `routePlan[].percent` - Route split percentage

### Swap Response - Critical Fields
- `swapTransaction` - Base64 encoded transaction (MUST exist and be valid base64)
- `lastValidBlockHeight` - Transaction expiry (MUST be positive)
- `prioritizationFeeLamports` - Applied priority fee

## Metadata Fields

Fields prefixed with `_` (e.g., `_fixture_metadata`, `_request_params`) are **not part of the
API contract**. They exist purely for documentation and are stripped during test deserialization.
