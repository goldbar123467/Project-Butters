# Kyzlo-DEX (Butters) - Architecture Documentation

## Overview

Kyzlo-DEX is a production-grade Solana mean reversion trading bot built in Rust. It uses hexagonal architecture (ports and adapters) for clean separation of concerns and testability.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI ADAPTER                              │
│                    (clap argument parsing)                       │
│         Commands: run, status, quote, swap, backtest, resume     │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                    TRADING ORCHESTRATOR                          │
│              (src/application/orchestrator.rs)                   │
│   - Price fetching loop                                          │
│   - Strategy coordination                                        │
│   - Trade execution pipeline                                     │
│   - Balance guard integration                                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  DOMAIN LAYER │    │ STRATEGY PORT │    │ EXECUTION PORT│
│   (Pure Logic)│    │  (Signals)    │    │   (Swaps)     │
└───────┬───────┘    └───────┬───────┘    └───────┬───────┘
        │                    │                    │
        │              ┌─────┴─────┐              │
        │              │           │              │
        ▼              ▼           ▼              ▼
┌─────────────┐  ┌──────────┐ ┌─────────┐  ┌────────────┐
│  Position   │  │ Z-Score  │ │  Mean   │  │   JITO     │
│  Portfolio  │  │  Gate    │ │Reversion│  │  ADAPTER   │
│  Risk Mgmt  │  └──────────┘ └─────────┘  └─────┬──────┘
│  Signals    │                                   │
│  Balance    │                            ┌──────▼──────┐
│   Guard     │                            │   JUPITER   │
└─────────────┘                            │   CLIENT    │
                                           └──────┬──────┘
                                                  │
                                           ┌──────▼──────┐
                                           │   SOLANA    │
                                           │   CLIENT    │
                                           └─────────────┘
```

## Project Structure

```
kyzlo-dex/
├── Cargo.toml                 # Dependencies and package config
├── config.toml                # Main configuration
├── config/
│   └── devnet.toml           # Devnet testing config
├── src/
│   ├── main.rs               # CLI entry point
│   ├── lib.rs                # Public API exports
│   │
│   ├── domain/               # Pure business logic (no external deps)
│   │   ├── mod.rs
│   │   ├── signal.rs         # Trading signals with z-score confidence
│   │   ├── trade.rs          # Swap models with fee tracking
│   │   ├── portfolio.rs      # Holdings and P&L tracking
│   │   ├── risk.rs           # Position/exposure/leverage limits
│   │   ├── position.rs       # Long/Short position lifecycle
│   │   ├── known_programs.rs # Whitelist: 17 DEXs, 8 Jito tips, system programs
│   │   ├── tx_validator.rs   # Pre-sign security validation
│   │   └── balance_guard.rs  # Post-trade balance verification
│   │
│   ├── ports/                # Abstract interfaces (traits)
│   │   ├── mod.rs
│   │   ├── models.rs         # Shared types: PortError, Instrument, Order
│   │   ├── market_data.rs    # MarketDataPort trait
│   │   ├── strategy.rs       # StrategyPort trait
│   │   ├── execution.rs      # ExecutionPort trait
│   │   └── mocks.rs          # Test doubles
│   │
│   ├── adapters/             # Concrete implementations
│   │   ├── mod.rs
│   │   ├── solana/
│   │   │   ├── mod.rs
│   │   │   ├── wallet.rs     # Keypair management, signing
│   │   │   └── rpc.rs        # Async RPC client wrapper
│   │   │
│   │   ├── jupiter/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs     # HTTP client with retry logic
│   │   │   ├── quote.rs      # Quote request/response types
│   │   │   ├── swap.rs       # Swap request/response types
│   │   │   └── contract_tests.rs  # Golden fixture tests
│   │   │
│   │   ├── jito/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs     # Bundle submission client
│   │   │   ├── types.rs      # BundleStatus, BundleResult
│   │   │   ├── error.rs      # JitoError with retry classification
│   │   │   ├── execution.rs  # ExecutionPort impl (fail-closed)
│   │   │   └── config.rs     # Regional endpoints, tip accounts
│   │   │
│   │   ├── market_data/
│   │   │   ├── mod.rs
│   │   │   └── jupiter_price.rs  # Price API client
│   │   │
│   │   └── cli/
│   │       ├── mod.rs
│   │       └── commands.rs   # CLI command definitions
│   │
│   ├── strategy/
│   │   ├── mod.rs
│   │   ├── mean_reversion.rs # Main strategy implementation
│   │   ├── zscore_gate.rs    # Rolling z-score calculator
│   │   └── params.rs         # StrategyConfig, RiskConfig
│   │
│   ├── config/
│   │   ├── mod.rs
│   │   └── loader.rs         # TOML loading with env overrides
│   │
│   └── application/
│       ├── mod.rs
│       └── orchestrator.rs   # TradingOrchestrator main loop
│
├── fixtures/
│   └── jupiter/              # API response fixtures for testing
│
├── analysis/
│   └── jupiter_api.md        # API documentation analysis
│
└── docs/
    └── ports.md              # Port interface documentation
```

## Domain Layer

### Core Components

| Module | Structs/Enums | Purpose |
|--------|---------------|---------|
| `signal.rs` | `Signal`, `SignalType` | Trading signals with z-score derived confidence |
| `trade.rs` | `Trade`, `Fee`, `TradeResult` | Swap models with fee tracking and simulation |
| `portfolio.rs` | `Portfolio`, `Holding` | Position tracking and P&L calculation |
| `risk.rs` | `RiskLimits`, `RiskCheck` trait | Position size, exposure, leverage validation |
| `position.rs` | `Position`, `Side`, `Status` | Long/Short position lifecycle management |
| `known_programs.rs` | Constants | Whitelists: 17 DEX programs, 8 Jito tips, 6 system programs |
| `tx_validator.rs` | `TransactionValidator` | Pre-sign validation blocks unauthorized transfers |
| `balance_guard.rs` | `BalanceGuard`, `GuardStatus` | Post-trade balance verification, auto-halt on anomalies |

### Security Flow

```
1. Signal Generation
   Signal (with z-score confidence)
        ↓
2. Position Management
   Position (Long/Short, Open/Closed)
        ↓
3. Trade Execution
   Trade (input → output, fees)
        ↓
4. Pre-Sign Security
   TransactionValidator ← uses KnownPrograms whitelist
        ↓
5. Balance Snapshot
   BalanceGuard.capture_pre_trade()
        ↓
6. Execute on Chain
        ↓
7. Post-Trade Validation
   BalanceGuard.validate_post_trade()
        ↓
8. Risk Check
   RiskLimits.validate_position_size()
```

## Strategy Implementation

### Mean Reversion with Z-Score Gate

**Entry Logic:**
- LONG when z-score < -z_threshold (oversold, price below mean)
- SHORT when z-score > +z_threshold (overbought, price above mean)

**Exit Logic:**
- Take profit: price moves favorably by take_profit_pct
- Stop loss: price moves against by stop_loss_pct
- Mean reversion: z-score crosses exit threshold toward 0
- Time stop: position held longer than time_stop_hours

**Risk Controls:**
- Max position size: 5% of portfolio
- Daily trade limit: 20 trades
- Daily loss limit: 3% (circuit breaker)
- Cooldown: 120 seconds between trades

### Default Parameters

```toml
[strategy]
lookback_period = 45        # Rolling window for mean/std
z_threshold = 1.8           # Entry threshold (±1.8 std devs)
z_exit_threshold = 0.2      # Exit deadband near mean
cooldown_seconds = 120      # 2 min between trades

[risk]
max_position_pct = 5.0      # 5% per trade
stop_loss_pct = 0.5         # 0.5% stop loss
take_profit_pct = 0.8       # 0.8% take profit
max_daily_trades = 20
max_daily_loss_pct = 3.0    # Circuit breaker
time_stop_hours = 1         # Exit after 1 hour
trade_size_sol = 0.1        # 0.1 SOL per signal
```

## Adapters

### Jupiter Integration

**Endpoints:**
- Quote: `GET /quote` - Get swap pricing and routing
- Swap: `POST /swap` - Build swap transaction

**Features:**
- Retry logic (3 attempts, exponential backoff)
- Rate limit handling
- Contract tests with golden fixtures
- Price impact validation (< 1%)

### Jito MEV Protection

**FAIL-CLOSED Policy:** If Jito submission fails, trade is NOT executed. No fallback to direct RPC.

**Bundle Flow:**
1. Build swap transaction via Jupiter
2. Create tip instruction to random Jito validator
3. Submit both as atomic bundle
4. Poll for bundle status (Landed/Failed/Dropped)

**Regional Endpoints:**
- NY (default): `https://ny.mainnet.block-engine.jito.wtf`
- Amsterdam, Frankfurt, Tokyo also available

### Solana RPC

**Client Features:**
- Async wrapper using `tokio::task::spawn_blocking`
- Thread-safe via `Arc<RpcClient>`
- Commitment level: "confirmed"

**Key Methods:**
- `get_balance()`, `get_token_account_balance()`
- `send_and_confirm_transaction()`
- `get_latest_blockhash()`

## CLI Commands

```bash
# Start trading loop
butters run --config config.toml --paper    # Paper trading
butters run --config config.toml --live --i-accept-losses  # Live trading

# Check status
butters status --detailed --format json

# Manual operations
butters quote SOL USDC 1.0 --slippage 50
butters swap SOL USDC 1.0 --dry-run

# Backtesting
butters backtest --pair SOL/USDC --days 30 --capital 10000

# Resume after halt
butters resume --reset-cumulative
```

## Safety Features

1. **MEV Protection**: All trades via Jito bundles (atomic, no sandwich attacks)
2. **Balance Guard**: Detects unexpected balance changes, auto-halts
3. **Transaction Validator**: Whitelists transfer destinations
4. **Preflight Checks**: Keypair permissions, network verification
5. **Paper Mode**: Full simulation without real transactions
6. **Daily Limits**: Trade count and loss circuit breakers

## Dependencies

**Core:**
- `tokio` 1.42 - Async runtime
- `solana-sdk` 2.1 - Solana primitives
- `solana-client` 2.1 - RPC client
- `reqwest` 0.12 - HTTP client

**Data:**
- `serde` 1.0 - Serialization
- `rust_decimal` 1.36 - Precise arithmetic
- `statrs` 0.17 - Statistics

**CLI:**
- `clap` 4.5 - Argument parsing
- `tracing` 0.1 - Structured logging

## Configuration

### Environment Variables
```bash
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_KEYPAIR_PATH=/path/to/keypair.json
JUPITER_API_KEY=your-api-key  # Optional, for higher rate limits
```

### Config Sections
- `[strategy]` - Z-score parameters, timeframe
- `[risk]` - Position limits, stop loss, daily limits
- `[tokens]` - Base/quote mint addresses
- `[jupiter]` - API URL, slippage, priority fees
- `[solana]` - RPC URL, commitment, keypair path
- `[jito]` - Enable/disable, region, tip amount
- `[logging]` - Log level, file output
- `[alerts]` - Discord/Telegram webhooks (optional)

## Testing

```bash
# Unit tests
cargo test

# Contract tests (Jupiter API fixtures)
cargo test contract_tests

# Live smoke tests (ignored by default)
cargo test -- --ignored
```

## License

MIT
