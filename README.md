# Butters - Mean Reversion Trading Bot

A conservative mean reversion trading strategy for Solana via Jupiter DEX.

## Features

- **Z-score Statistical Gating** - Uses 2.5 standard deviation threshold to identify high-probability mean reversion opportunities
- **Hexagonal Architecture** - Clean separation of domain, ports, and adapters for testability and maintainability
- **Paper Trading Mode** - Test strategies without risking real funds
- **Jupiter V6 Integration** - Optimal swap routing through Jupiter aggregator
- **Risk Management** - Built-in position sizing, stop losses, and daily loss limits

## Quick Start (Paper Trading)

### Prerequisites

- Rust 1.70+
- Solana CLI (for wallet management)

### Setup

1. Clone and build:
```bash
git clone <repository-url>
cd kyzlo-dex
cargo build --release
```

2. Create a Solana wallet (if you don't have one):
```bash
solana-keygen new --outfile ~/.config/solana/id.json
```

3. Copy and configure settings:
```bash
cp config.toml config/mainnet.toml
# Edit config/mainnet.toml with your settings
```

4. (Optional) Set up environment variables for API keys:
```bash
cp .env.example .env
# Add your JUPITER_API_KEY and SOLANA_RPC_URL
```

### Running

Start paper trading:
```bash
./target/release/butters run --paper
```

### Commands

```bash
# Start trading loop (paper mode)
butters run --paper

# Start trading loop (live mode - use with caution!)
butters run

# Check wallet status and balance
butters status

# Get a swap quote
butters quote SOL USDC 1.0

# Execute a swap (with confirmation)
butters swap SOL USDC 1.0

# Run backtesting
butters backtest --pair SOL/USDC --days 30
```

## Configuration

The `config.toml` file contains all strategy parameters:

```toml
[strategy]
lookback_period = 20    # Candles for rolling mean/std calculation
z_threshold = 2.0       # Z-score threshold (2.0 = moderate, 2.5 = conservative)
cooldown_seconds = 300  # Minimum time between trades

[risk]
max_position_pct = 5.0  # Maximum position size as % of portfolio
stop_loss_pct = 2.5     # Stop loss percentage
max_daily_trades = 10   # Maximum trades per day
max_daily_loss_pct = 3.0 # Circuit breaker: max daily loss

[jupiter]
slippage_bps = 50       # Slippage tolerance (0.5%)

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
keypair_path = "~/.config/solana/id.json"
```

**Important**: Never commit your keypair file or API keys. Use environment variables for sensitive data.

## Architecture

Butters follows hexagonal (ports & adapters) architecture:

```
┌─────────────────────────────────────────────────────────┐
│                      CLI ADAPTER                         │
└─────────────────────────┬───────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────┐
│                    ORCHESTRATOR                          │
│                   (application layer)                    │
└─────────────────────────┬───────────────────────────────┘
                          │
    ┌─────────────────────┼─────────────────────┐
    │                     │                     │
┌───▼───┐           ┌─────▼─────┐         ┌─────▼─────┐
│DOMAIN │           │ STRATEGY  │         │  MARKET   │
│ CORE  │           │   PORT    │         │   PORT    │
│       │           │           │         │           │
│Position│          │ ZScoreGate│         │ PriceFeed │
│Trade   │          │ MeanRevert│         │   OHLCV   │
│Portfolio│         └─────┬─────┘         └─────┬─────┘
│RiskLimits│              │                     │
└─────────┘         ┌─────▼─────┐         ┌─────▼─────┐
                    │ STRATEGY  │         │  JUPITER  │
                    │  ADAPTER  │         │  ADAPTER  │
                    └───────────┘         └─────┬─────┘
                                                │
                                          ┌─────▼─────┐
                                          │  SOLANA   │
                                          │  CLIENT   │
                                          └───────────┘
```

### Directory Structure

```
src/
├── main.rs              # CLI entrypoint
├── domain/              # Pure business logic (zero external deps)
├── ports/               # Trait definitions (interfaces)
├── strategy/            # Mean reversion & z-score implementation
├── adapters/            # External integrations
│   ├── jupiter/         # Jupiter V6 API client
│   ├── solana/          # Solana RPC & wallet
│   └── cli/             # Command-line interface
├── application/         # Orchestration & use cases
└── config/              # Configuration loading
```

## Strategy Overview

The mean reversion strategy identifies oversold conditions using z-score analysis:

```
z_score = (current_price - rolling_mean) / rolling_std

LONG:  z_score < -2.5  (oversold - price is 2.5 std devs below mean)
EXIT:  z_score > 0     (price reverts to mean)
```

With a z-threshold of 2.5, only ~1.2% of price movements trigger trades, focusing on high-probability setups where extreme deviations historically revert to the mean with 65-75% probability.

## Development

Run tests:
```bash
cargo test
```

Run with debug logging:
```bash
cargo run -- --debug run --paper
```

## License

MIT
