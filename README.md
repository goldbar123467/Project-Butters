# Butters - Mean Reversion Trading Bot

[![Build Status](https://img.shields.io/github/actions/workflow/status/goldbar123467/kyzlo-dex/ci.yml?branch=main&style=flat-square&logo=github&label=build)](https://github.com/goldbar123467/kyzlo-dex/actions)
[![Tests](https://img.shields.io/badge/tests-107%20passing-brightgreen?style=flat-square&logo=checkmarx&logoColor=white)](https://github.com/goldbar123467/kyzlo-dex)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/solana--sdk-2.1-blueviolet?style=flat-square&logo=solana)](https://solana.com/)
[![Jupiter](https://img.shields.io/badge/jupiter-v6-00D18C?style=flat-square)](https://jup.ag/)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

> Conservative mean reversion trading strategy for Solana via Jupiter DEX aggregator. Built in Rust with hexagonal architecture for reliability and testability.

---

## One-Shot Quickstart

Get from zero to running in 6 copy-paste steps. Each step shows expected output so you know it worked.

### Step 1: Check Prerequisites

```bash
rustc --version && cargo --version && solana --version
```

**Expected output:**
```
rustc 1.70.0 (or higher)
cargo 1.70.0 (or higher)
solana-cli 1.18.x (or higher)
```

> If any command fails, see [Troubleshooting](#troubleshooting) below.

### Step 2: Clone and Build

```bash
git clone https://github.com/goldbar123467/kyzlo-dex.git
cd kyzlo-dex
cargo build --release
```

**Expected output:**
```
   Compiling butters v0.1.0
    Finished release [optimized] target(s) in 45.23s
```

### Step 3: Generate Devnet Wallet + Airdrop

```bash
solana-keygen new --outfile ~/.config/solana/devnet.json --no-bip39-passphrase
solana config set --url devnet --keypair ~/.config/solana/devnet.json
solana airdrop 2
```

**Expected output:**
```
Wrote new keypair to ~/.config/solana/devnet.json
Config File: ~/.config/solana/cli/config.yml
Requesting airdrop of 2 SOL
Signature: 5abc123...
2 SOL
```

### Step 4: Verify Balance

```bash
solana balance
```

**Expected output:**
```
2 SOL
```

### Step 5: Get Your First Quote

```bash
cargo run --release -- quote SOL USDC 0.1
```

**Expected output:**
```
Quote received:
  Input: 0.1 SOL
  Output: ~15.23 USDC
  Price Impact: 0.01%
  Route: SOL -> USDC via Raydium
```

### Step 6: Run Paper Trading

```bash
cargo run --release -- run --paper
```

**Expected output:**
```
[INFO] Starting paper trading mode...
[INFO] Loaded config from config.toml
[INFO] Watching SOL/USDC pair
[INFO] Current z-score: 0.42 (within bounds, no trade)
[INFO] Tick 1 complete. Portfolio: 2.0 SOL, 0.0 USDC
```

You are now running Butters in paper trading mode. No real funds are at risk.

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `rustc: command not found` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` then restart terminal |
| `solana: command not found` | Install Solana CLI: `sh -c "$(curl -sSfL https://release.solana.com/stable/install)"` then add to PATH |
| `linker 'cc' not found` | Install build tools: `sudo apt install build-essential` (Ubuntu) or `xcode-select --install` (macOS) |
| `RPC rate limit exceeded` | Wait 60 seconds, or use a private RPC endpoint in config.toml |
| `Airdrop failed` | Devnet faucet may be dry. Try: `solana airdrop 1` (smaller amount) or wait and retry |
| `insufficient funds for rent` | You need at least 0.01 SOL for account rent. Request another airdrop. |

---

## Features

| Feature | Description |
|---------|-------------|
| :chart_with_upwards_trend: **Z-Score Statistical Gating** | Only trades when price deviates significantly from rolling mean (configurable threshold) |
| :hexagon: **Hexagonal Architecture** | Clean separation of domain logic, ports, and adapters for easy testing and swapping |
| :page_facing_up: **Paper Trading Mode** | Simulate trades without risking real funds - perfect for strategy tuning |
| :rocket: **Jupiter V6 Integration** | Access to 20+ Solana DEXs through Jupiter's smart routing aggregator |
| :shield: **Risk Management** | Position sizing, max drawdown limits, and automatic stop-loss triggers |

---

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
rpc_url = "https://api.devnet.solana.com"  # Use devnet for testing
keypair_path = "~/.config/solana/devnet.json"
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `lookback_period` | 20 | Number of candles for calculating rolling mean and standard deviation |
| `z_threshold` | 2.0 | Z-score threshold to trigger trades (higher = fewer, more confident trades) |
| `max_position_pct` | 5.0 | Maximum position size as percentage of portfolio |
| `slippage_bps` | 50 | Maximum allowed slippage in basis points (50 = 0.5%) |

**Important**: Never commit your keypair file or API keys. Use environment variables for sensitive data.

---

## Architecture

Butters follows hexagonal (ports & adapters) architecture:

```
                    +------------------+
                    |   CLI Adapter    |
                    +--------+---------+
                             |
                    +--------v---------+
                    |   ORCHESTRATOR   |
                    | (application)    |
                    +--------+---------+
                             |
         +-------------------+-------------------+
         |                   |                   |
+--------v-------+  +--------v-------+  +--------v-------+
|  DOMAIN CORE   |  |  STRATEGY      |  |  MARKET        |
|                |  |  PORT          |  |  PORT          |
|  - Position    |  |  - ZScoreGate  |  |  - PriceFeed   |
|  - Trade       |  |  - MeanRevert  |  |  - OHLCV       |
|  - Portfolio   |  +--------+-------+  +--------+-------+
|  - RiskLimits  |           |                   |
+----------------+  +--------v-------+  +--------v-------+
                    |  STRATEGY      |  |  JUPITER       |
                    |  ADAPTER       |  |  ADAPTER       |
                    +----------------+  +--------+-------+
                                                 |
                                        +--------v-------+
                                        |  SOLANA        |
                                        |  CLIENT        |
                                        +----------------+
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

---

## Strategy Overview

The mean reversion strategy identifies oversold/overbought conditions using z-score analysis:

### The Z-Score Formula

```
z_score = (current_price - rolling_mean) / rolling_std
```

| Z-Score | Interpretation | Action |
|---------|----------------|--------|
| z < -2.0 | Price is unusually LOW (oversold) | BUY (expect price to rise back to mean) |
| z > +2.0 | Price is unusually HIGH (overbought) | SELL (expect price to fall back to mean) |
| -2.0 < z < +2.0 | Price is normal | HOLD (no trade) |

### Example

```
Rolling 20-period mean: $150.00
Standard deviation: $5.00
Current price: $138.00

z = (138 - 150) / 5 = -2.4

Since z < -2.0, the strategy signals BUY (oversold)
```

With a z-threshold of 2.5, only ~1.2% of price movements trigger trades, focusing on high-probability setups where extreme deviations historically revert to the mean with 65-75% probability.

---

## FAQ

<details>
<summary><strong>1. What is this project?</strong></summary>

Butters is an automated trading bot that executes mean reversion trades on the Solana blockchain. It monitors price movements, calculates statistical deviations, and automatically buys when prices are unusually low or sells when prices are unusually high. The name "Butters" reflects its conservative, steady approach to trading.

</details>

<details>
<summary><strong>2. What is Solana?</strong></summary>

Solana is a high-performance blockchain that can process thousands of transactions per second with sub-second finality. Unlike Ethereum, Solana has very low transaction fees (typically under $0.01), making it ideal for trading bots that execute many small trades. Solana uses a Proof-of-Stake consensus mechanism and its native currency is SOL.

</details>

<details>
<summary><strong>3. What is Jupiter?</strong></summary>

Jupiter is a DEX (Decentralized Exchange) aggregator on Solana. Instead of connecting to one exchange, Jupiter routes your trade through 20+ exchanges (Raydium, Orca, Serum, etc.) to find the best price. Think of it like a flight aggregator that searches multiple airlines - Jupiter searches multiple DEXs to get you the best swap rate with minimal slippage.

</details>

<details>
<summary><strong>4. What is mean reversion?</strong></summary>

Mean reversion is a trading strategy based on the observation that prices tend to return to their average over time. If a price spikes unusually high, it often falls back. If it drops unusually low, it often bounces back. Butters exploits this pattern by buying during dips and selling during spikes, profiting from the "reversion to the mean."

</details>

<details>
<summary><strong>5. What is a z-score?</strong></summary>

A z-score measures how many standard deviations a value is from the average. A z-score of 0 means the price equals the average. A z-score of +2 means the price is 2 standard deviations above average (unusually high). A z-score of -2 means it is 2 standard deviations below (unusually low). In statistics, about 95% of values fall within +/- 2 standard deviations, so anything outside that range is considered significant.

</details>

<details>
<summary><strong>6. What are the prerequisites?</strong></summary>

You need three things installed:

1. **Rust** (1.70+): The programming language Butters is written in
2. **Solana CLI**: Command-line tools for interacting with Solana
3. **A Solana wallet**: A keypair file that holds your funds

Optional but recommended:
- A private RPC endpoint (free tier from Helius, QuickNode, or Triton)
- Basic understanding of trading concepts

</details>

<details>
<summary><strong>7. Is this safe to use with real money?</strong></summary>

**Use at your own risk.** Butters is experimental software. While it includes risk management features (max drawdown, position limits), cryptocurrency trading is inherently risky. We strongly recommend:

1. Start with paper trading mode (no real funds)
2. Test extensively on devnet with fake SOL
3. If using real funds, start with very small amounts you can afford to lose
4. Never invest money you cannot afford to lose

</details>

<details>
<summary><strong>8. What are lamports?</strong></summary>

Lamports are the smallest unit of SOL, similar to how cents are the smallest unit of dollars. 1 SOL = 1,000,000,000 (one billion) lamports. When specifying trade amounts in the config, you use lamports. For example, 100,000,000 lamports = 0.1 SOL.

</details>

<details>
<summary><strong>9. What are token mints?</strong></summary>

On Solana, every token has a unique "mint address" that identifies it. Think of it like a stock ticker symbol, but as a long string. Common mints:

| Token | Mint Address |
|-------|--------------|
| SOL | `So11111111111111111111111111111111111111112` |
| USDC | `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` |
| USDT | `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB` |

You configure which tokens to trade using these mint addresses.

</details>

<details>
<summary><strong>10. What is slippage?</strong></summary>

Slippage is the difference between the expected price of a trade and the actual executed price. It happens because prices can change between when you request a quote and when the transaction confirms. Slippage is measured in basis points (bps), where 100 bps = 1%. Setting `slippage_bps = 50` means you accept up to 0.5% worse than the quoted price.

</details>

<details>
<summary><strong>11. What is an RPC endpoint?</strong></summary>

An RPC (Remote Procedure Call) endpoint is a server that lets you interact with the Solana blockchain. It is like an API for reading blockchain data and submitting transactions. The public endpoints (api.devnet.solana.com) are free but rate-limited. For production use, you should use a private RPC provider like Helius, QuickNode, or Triton, which offer free tiers with higher limits.

</details>

<details>
<summary><strong>12. How do I get test SOL for devnet?</strong></summary>

Devnet SOL is free and has no real value. Get it via:

```bash
# Method 1: CLI airdrop
solana airdrop 2

# Method 2: Web faucet
# Visit https://faucet.solana.com and paste your wallet address
```

If airdrops fail (faucet is dry), wait a few minutes and try again with a smaller amount (1 SOL instead of 2).

</details>

<details>
<summary><strong>13. What is paper trading mode?</strong></summary>

Paper trading simulates trades without actually executing them on the blockchain. The bot tracks a virtual portfolio and logs what trades it *would* have made. This lets you test and tune your strategy without risking real funds. Enable it with the `--paper` flag. Always paper trade first before using real money.

</details>

<details>
<summary><strong>14. How do I understand the config file?</strong></summary>

The config.toml has four sections:

| Section | What it controls |
|---------|------------------|
| `[strategy]` | Trading logic: z-score threshold, lookback period, cooldown time |
| `[risk]` | Safety limits: max position size, stop loss, daily loss limit |
| `[jupiter]` | DEX settings: slippage tolerance |
| `[solana]` | Network: RPC URL, wallet keypair path |

Start with the defaults and adjust based on your paper trading results.

</details>

<details>
<summary><strong>15. Why hexagonal architecture?</strong></summary>

Hexagonal architecture (also called "ports and adapters") keeps the core trading logic separate from external dependencies like Jupiter or Solana. Benefits:

1. **Testability**: Test the strategy with mock data without hitting real APIs
2. **Flexibility**: Swap Jupiter for another DEX aggregator without rewriting strategy code
3. **Reliability**: External API changes do not break your core logic
4. **Contract Testing**: Use recorded API responses (fixtures) to test without network calls

The `src/adapters/jupiter/` folder contains the Jupiter integration, which can be swapped out without touching `src/domain/`.

</details>

---

## Contract Testing

Butters uses **contract testing** with recorded API fixtures to ensure reliability without depending on live APIs during tests.

```
fixtures/
└── jupiter/
    ├── quote_sol_usdc.json      # Recorded Jupiter quote response
    ├── swap_instructions.json   # Recorded swap transaction data
    └── README.md                # Fixture documentation
```

This approach:
- Makes tests fast and deterministic
- Works offline
- Catches API contract changes early
- Lives in `fixtures/` directory

To update fixtures after Jupiter API changes:

```bash
cargo test --features record-fixtures
```

---

## Development

### Run Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_zscore_calculation
```

### Debug Mode

```bash
# Build with debug symbols
cargo build

# Run with verbose logging
RUST_LOG=debug cargo run -- run --paper
```

### Commands Reference

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

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

<p align="center">
  <sub>Built with Rust. Powered by Jupiter. Running on Solana.</sub>
</p>
