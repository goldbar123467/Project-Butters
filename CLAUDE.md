# CLAUDE.md — Jupiter Mean Reversion DEX

## Project Identity
- **Codename**: Butters
- **Architecture**: Hexagonal (Ports & Adapters)
- **Language**: Rust
- **Runtime**: CLI on Solana Mainnet via Jupiter Aggregator
- **Strategy**: Conservative Mean Reversion with Z-Score Gating

## Mission
Build a production CLI trading engine that:
- Connects to Solana via Jupiter aggregator for optimal swap routing
- Executes mean reversion strategy with 1.5% trigger frequency
- Uses z-score statistical gating to filter noise and maximize win rate
- Hexagonal architecture for testability and adapter swapping

## Hexagonal Architecture
```
                         ┌─────────────────┐
                         │   CLI ADAPTER   │
                         └────────┬────────┘
                                  │
                         ┌────────▼────────┐
                         │  ORCHESTRATOR   │
                         │  (application)  │
                         └────────┬────────┘
                                  │
        ┌─────────────────────────┼─────────────────────────┐
        │                         │                         │
┌───────▼───────┐       ┌─────────▼─────────┐     ┌─────────▼─────────┐
│  DOMAIN CORE  │       │   STRATEGY PORT   │     │    MARKET PORT    │
│  - Position   │       │   - ZScoreGate    │     │    - PriceFeed    │
│  - Trade      │       │   - MeanReversion │     │    - OHLCV        │
│  - Portfolio  │       └─────────┬─────────┘     └─────────┬─────────┘
│  - RiskLimits │                 │                         │
└───────────────┘       ┌─────────▼─────────┐     ┌─────────▼─────────┐
                        │ STRATEGY ADAPTER  │     │  JUPITER ADAPTER  │
                        └───────────────────┘     └─────────┬─────────┘
                                                            │
                                                  ┌─────────▼─────────┐
                                                  │  SOLANA CLIENT    │
                                                  │  - RPC            │
                                                  │  - Wallet         │
                                                  │  - Tx Builder     │
                                                  └───────────────────┘
```

## Directory Structure
```
src/
├── main.rs                     # CLI entrypoint
├── domain/                     # CORE - Pure business logic, zero deps
│   ├── mod.rs
│   ├── position.rs
│   ├── trade.rs
│   ├── portfolio.rs
│   └── risk.rs
├── ports/                      # PORTS - Traits only
│   ├── mod.rs
│   ├── market.rs               # trait MarketDataPort
│   ├── execution.rs            # trait ExecutionPort
│   └── strategy.rs             # trait StrategyPort
├── adapters/                   # ADAPTERS - Implementations
│   ├── mod.rs
│   ├── jupiter/
│   │   ├── mod.rs
│   │   ├── client.rs           # Jupiter V6 API
│   │   ├── quote.rs
│   │   └── swap.rs
│   ├── solana/
│   │   ├── mod.rs
│   │   ├── rpc.rs
│   │   └── wallet.rs
│   └── cli/
│       ├── mod.rs
│       └── commands.rs
├── application/                # USE CASES
│   ├── mod.rs
│   └── orchestrator.rs
└── strategy/                   # SIGNAL GENERATION
    ├── mod.rs
    ├── mean_reversion.rs
    ├── zscore_gate.rs
    └── params.rs
```

## Mean Reversion Strategy

### Z-Score Formula
```
z_score = (current_price - rolling_mean) / rolling_std

LONG:  z_score < -2.5  (oversold)
EXIT:  z_score > +2.5  (overbought) OR take_profit OR stop_loss
```

### Parameters (1.5% Trigger Rate Target)
```toml
lookback_period = 50        # candles for rolling stats
z_threshold = 2.5           # 2.5 std devs = conservative
min_volume_percentile = 60  # volume filter
max_spread_bps = 30         # spread filter
cooldown_seconds = 300      # 5 min between trades

# Risk
max_position_pct = 5.0      # 5% max per trade
stop_loss_pct = 2.0
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
```

### Why This Works
- Z-threshold 2.5 = only 1.2% of data points in normal distribution
- Combined with volume/spread filters → ~1.5% actual trigger rate
- Extreme deviations revert to mean with 65-75% probability
- Conservative sizing preserves capital for high-conviction setups

## Jupiter Integration

### Endpoints
```
Quote:  GET https://public.jupiterapi.com/v6/quote
Swap:   POST https://public.jupiterapi.com/v6/swap
Price:  GET https://price.jup.ag/v6/price
```

> **Note:** `quote-api.jup.ag` is deprecated. Use `public.jupiterapi.com` for all quote and swap endpoints.

### Quote Request
```rust
struct QuoteRequest {
    input_mint: String,
    output_mint: String,
    amount: u64,              // in lamports/base units
    slippage_bps: u16,        // e.g., 50 = 0.5%
    only_direct_routes: bool, // false for best routing
}
```

### Swap Flow
1. Fetch quote from Jupiter
2. Build swap transaction (Jupiter returns serialized tx)
3. Sign with wallet keypair
4. Submit to Solana RPC
5. Confirm transaction

## Solana Client

### Dependencies (Actual Cargo.toml)
```toml
# Async runtime
tokio = { version = "1.42", features = ["full"] }
async-trait = "0.1"

# Solana SDK
solana-sdk = "2.1"
solana-client = "2.1"
solana-transaction-status = "2.1"

# Jupiter API
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }

# Math & Statistics
rust_decimal = "1.36"
statrs = "0.17"
```

### Key Operations
- `RpcClient::new(rpc_url)` - connect to RPC
- `Keypair::from_bytes()` - load wallet
- `client.send_and_confirm_transaction()` - execute swap
- `client.get_token_account_balance()` - check balances

## CLI Commands
```bash
# Start trading loop
butters run --config mainnet.toml

# Check status
butters status

# Manual operations
butters quote SOL USDC 1.0
butters swap SOL USDC 1.0 --slippage 50

# Backtest
butters backtest --pair SOL/USDC --days 30
```

## Build Order
1. **Domain first** - Position, Trade, Portfolio, Risk (pure structs, no deps)
2. **Ports second** - Define traits for Market, Execution, Strategy
3. **Strategy third** - MeanReversion, ZScoreGate implementations
4. **Adapters fourth** - Jupiter client, Solana client, CLI
5. **Orchestrator last** - Wire everything together

## Testing Strategy
- **Domain**: Unit tests, pure functions
- **Strategy**: Backtest against historical data
- **Adapters**: Integration tests with devnet
- **Full system**: Paper trading mode before mainnet

## Config File (config/mainnet.toml)
```toml
[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
keypair_path = "~/.config/solana/id.json"

[jupiter]
api_url = "https://public.jupiterapi.com/v6"
slippage_bps = 50

[strategy]
lookback_period = 50
z_threshold = 2.5
min_volume_percentile = 60
max_spread_bps = 30
cooldown_seconds = 300

[risk]
max_position_pct = 5.0
stop_loss_pct = 2.0
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0

[tokens]
base = "So11111111111111111111111111111111111111112"   # SOL
quote = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" # USDC
```

## Build Progress

### ✅ Phase 1: Domain Layer (COMPLETE)
- `src/domain/mod.rs` - Module exports
- `src/domain/types.rs` - Core types (TokenAmount, Price, Timestamp)
- `src/domain/position.rs` - Position/Holding management
- `src/domain/trade.rs` - Trade execution records
- `src/domain/portfolio.rs` - Portfolio state (8 tests)
- `src/domain/risk.rs` - Risk limits enforcement (8 tests)
- `src/domain/signal.rs` - Trading signals with confidence (8 tests)
- **24 domain tests passing** ✓

### ✅ Phase 2: Ports Layer (COMPLETE)
- `src/ports/mod.rs` - Trait exports
- `src/ports/market_data.rs` - MarketDataPort trait + OHLCV types
- `src/ports/execution.rs` - ExecutionPort trait + Jupiter types
- `src/ports/strategy.rs` - StrategyPort trait + signal types
- `src/ports/models.rs` - Shared types (Instrument, Order, etc.)
- **Compiles clean** ✓

### ✅ Phase 3: Strategy Layer (COMPLETE)
- `src/strategy/mod.rs` - Module exports
- `src/strategy/params.rs` - StrategyConfig, RiskConfig, FilterConfig (6 tests)
- `src/strategy/zscore_gate.rs` - ZScoreGate with rolling stats (10 tests)
- `src/strategy/mean_reversion.rs` - MeanReversionStrategy + StrategyPort impl (10 tests)
- **26 strategy tests passing** ✓
- **50 total tests passing** ✓

### ✅ Phase 4: Adapters (COMPLETE)
- `src/adapters/mod.rs` - Module exports
- `src/adapters/jupiter/mod.rs` - Jupiter module exports
- `src/adapters/jupiter/client.rs` - JupiterClient with ExecutionPort impl (3 tests)
- `src/adapters/jupiter/quote.rs` - QuoteRequest/QuoteResponse types (4 tests)
- `src/adapters/jupiter/swap.rs` - SwapRequest/SwapResponse/SwapResult (11 tests)
- `src/adapters/solana/mod.rs` - Solana module exports
- `src/adapters/solana/rpc.rs` - SolanaClient RPC wrapper (2 tests)
- `src/adapters/solana/wallet.rs` - WalletManager keypair handling (8 tests)
- `src/adapters/cli/mod.rs` - CLI module exports
- `src/adapters/cli/commands.rs` - CliApp with clap derive (12 tests)
- **40 adapter tests passing** ✓
- **90 total tests passing** ✓

### ✅ Phase 5: Orchestrator (COMPLETE)
- `src/config/mod.rs` - Config module exports
- `src/config/loader.rs` - TOML config loading with validation (7 tests)
- `src/application/mod.rs` - Application module exports
- `src/application/orchestrator.rs` - TradingOrchestrator with trading loop (8 tests)
- `src/adapters/market_data/mod.rs` - Market data module exports
- `src/adapters/market_data/jupiter_price.rs` - Jupiter price API client (2 tests)
- `src/main.rs` - Full CLI with async runtime and graceful shutdown
- **17 orchestrator tests passing** ✓
- **107 total tests passing** ✓

### 🚀 Phase 6: Integration & Testing (NEXT)
- Integration tests with devnet
- Paper trading mode testing
- Mainnet deployment preparation

---

## Actual Directory Structure (Current)
```
src/
├── main.rs                     # Async CLI entrypoint with Ctrl+C handling
├── domain/
│   ├── mod.rs
│   ├── types.rs                # TokenAmount, Price, Timestamp
│   ├── position.rs             # Holding (renamed from Position)
│   ├── trade.rs                # Trade, TradeType, TradeStatus
│   ├── portfolio.rs            # Portfolio with holdings map
│   ├── risk.rs                 # RiskLimits, RiskCheck
│   └── signal.rs               # Signal with confidence scoring
├── ports/
│   ├── mod.rs
│   ├── market_data.rs          # MarketDataPort trait
│   ├── execution.rs            # ExecutionPort trait
│   ├── strategy.rs             # StrategyPort trait
│   └── models.rs               # Shared types
├── strategy/
│   ├── mod.rs
│   ├── params.rs               # StrategyConfig, RiskConfig, FilterConfig
│   ├── zscore_gate.rs          # ZScoreGate rolling stats calculator
│   └── mean_reversion.rs       # MeanReversionStrategy + StrategyPort impl
├── config/
│   ├── mod.rs                  # Config module exports
│   └── loader.rs               # TOML config with validation
├── application/
│   ├── mod.rs                  # Application module exports
│   └── orchestrator.rs         # TradingOrchestrator trading loop
├── adapters/
│   ├── mod.rs
│   ├── jupiter/
│   │   ├── mod.rs
│   │   ├── client.rs           # JupiterClient ExecutionPort impl
│   │   ├── quote.rs            # QuoteRequest/QuoteResponse
│   │   └── swap.rs             # SwapRequest/SwapResponse
│   ├── solana/
│   │   ├── mod.rs
│   │   ├── rpc.rs              # SolanaClient RPC wrapper
│   │   └── wallet.rs           # WalletManager keypair handling
│   ├── cli/
│   │   ├── mod.rs
│   │   └── commands.rs         # CliApp with clap derive
│   └── market_data/
│       ├── mod.rs
│       └── jupiter_price.rs    # Jupiter price API client
analysis/
└── jupiter_api.md              # Jupiter API notes
docs/
└── ports.md                    # Ports layer documentation
```

---

## Agent Loop Instructions
When running as self-looping agent:
1. Build component per build order
2. After each component, run `cargo check`
3. If errors, fix before proceeding
4. Write tests alongside implementation
5. Integration test on devnet before mainnet wiring
