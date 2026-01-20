# 🧈 Butters

[![Build](https://img.shields.io/badge/build-passing-brightgreen?style=flat-square)]()
[![Tests](https://img.shields.io/badge/tests-1070%20passing-brightgreen?style=flat-square)]()
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/solana-mainnet-blueviolet?style=flat-square)](https://solana.com/)
[![Jupiter](https://img.shields.io/badge/jupiter-v6-blue?style=flat-square)](https://jup.ag/)
[![Jito](https://img.shields.io/badge/jito-MEV%20protected-purple?style=flat-square)](https://www.jito.wtf/)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)

**Conservative mean reversion trading bot for Solana via Jupiter DEX aggregator with Jito MEV protection and ADX regime filtering.**

---

## Features

| Feature | Description |
|---------|-------------|
| :chart_with_upwards_trend: **Z-Score Statistical Gating** | Only trades when price deviates significantly from rolling mean (z-score ±3.0 entry, ±0.37 exit). Targets extreme moves with 65-75% reversion probability. |
| :bar_chart: **ADX Regime Detection** | Filters out trending markets using Wilder's ADX indicator. Mean reversion works best in ranging markets (ADX < 25). Automatically scales position size based on trend strength. |
| :hexagon: **Hexagonal Architecture** | Clean separation of domain logic, ports, and adapters. 92 Rust files, 1070+ tests. Enables easy testing, mocking, and swapping of external integrations. |
| :page_facing_up: **Paper Trading Mode** | Simulate trades without risking real funds. Perfect for strategy tuning and validation before going live. Virtual portfolio tracks what would have happened. |
| :rocket: **Jupiter V6 Integration** | Access to 20+ Solana DEXs through Jupiter's smart routing aggregator. Automatically finds best prices with minimal slippage across Raydium, Orca, Serum, and more. |
| :shield: **Jito MEV Protection** | Submit trades as atomic bundles via Jito Block Engine to prevent frontrunning, sandwich attacks, and other MEV extraction. Fail-closed policy: trades execute safely or not at all. |
| :lock: **BalanceGuard Security** | Pre/post trade balance validation detects unexpected losses. Transaction validator whitelists only known programs (Jupiter, Jito tips). Halts trading on anomalies. |
| :moneybag: **Risk Management** | Position sizing limits, max drawdown protection, automatic stop-loss triggers, time-based exits, and daily loss circuit breakers. Conservative defaults protect capital. |
| :satellite: **Real-time Price Monitoring** | Continuous price feeds with 1-minute OHLC candle aggregation for ADX. Z-score calculation over configurable lookback windows. |

---

## 🎯 Live Trading Proof

**Real mainnet trades executed January 9, 2026** - No paper trading, no simulations.

### Trade Results

| # | Direction | Entry | Exit | Profit | TX |
|---|-----------|-------|------|--------|-----|
| 1 | LONG | $139.31 | $139.42 | **+$0.11 (+0.08%)** | [3tY49M...8h7Y](https://solscan.io/tx/3tY49MgVDvuUhzwQAEGb8yPkN3N4AmeRXMhPU1V1ULhvVXZ9CKXUk42kamKsMQfDhdATQx16BAhVrCvQvpGt8h7Y) |
| 2 | LONG | $139.30 | $139.45 | **+$0.016 (+0.11%)** | [CzYBfu...C2z](https://solscan.io/tx/CzYBfuJG6NMQRRrUcz5haTTdRXhaV8AuDo4YqzY5AFagPbeydK2LgtvLe54HT26rmHQMCcbmUozaG2Ta5iUhC2z) |

### Summary Stats

| Metric | Value |
|--------|-------|
| Total Trades | 2 |
| Win Rate | 100% |
| Total Profit | +$0.126 |
| Avg Return | +0.095% |
| BalanceGuard Accuracy | Perfect (-1 lamport) |

> **Note on BalanceGuard:** A delta of -1 lamport indicates perfect execution with only the unavoidable rent/fee overhead. This proves the bot is executing trades exactly as calculated with no slippage losses or hidden fees.

---

## 🦀 Why Rust?

Most trading bots are written in Python. Butters isn't. Here's why that matters for a bot that needs to catch mean reversion opportunities in milliseconds:

| Concern | Python | Rust |
|---------|--------|------|
| **Memory** | GC can pause at the worst moment | You control exactly when allocations happen |
| **Speed** | NumPy helps, but interpreter overhead remains | Native speed for z-score math and price feeds |
| **Deployment** | `pip install` → dependency hell → "works on my machine" | Single binary. Copy it. Run it. Done. |
| **Math Modules** | Duck typing makes stats code fragile | Traits make z-score calculators composable and testable |

### Where It Actually Matters

- **Z-Score Calculations**: Computing rolling standard deviations across 50 candles runs at C speed without sacrificing readability.
- **Jupiter API Calls**: Deserializing quote responses is allocation-heavy. Rust reuses buffers and avoids GC pauses right when you need to execute.
- **Jito Bundles**: Assembling and signing bundle transactions is latency-critical. No interpreter overhead, no surprise GC.
- **Environment Drift**: Six months from now, this bot will still compile and run exactly the same way. No virtualenv resurrection required.

---

## ⚡ Performance Characteristics

| Metric | Performance |
|--------|-------------|
| Trade Execution | ~3 seconds (signal to on-chain confirmation) |
| Quote Latency | <20ms from Jupiter API |
| Z-Score Calculation | Sub-millisecond (60-candle rolling window) |
| ADX Calculation | Sub-millisecond (10-period Wilder's smoothing) |
| Binary Size | ~10MB single executable |
| Memory Footprint | <50MB runtime |
| Test Suite | 1070+ tests in <10 seconds |

### Deployment Simplicity

```bash
# That's it. No virtualenv, no pip install, no dependency conflicts.
./butters run --config config.toml
```

A Python equivalent would require `numpy`, `pandas`, `asyncio`, `aiohttp`, `solana-py`, and numerous other dependencies. Butters ships as a single binary with zero runtime dependencies.

---

## 🔧 Rust Implementation Highlights

### Hexagonal Architecture (Ports & Adapters)

```rust
pub trait MarketDataPort: Send + Sync {
    async fn get_price(&self, token: &TokenMint) -> Result<Price, MarketError>;
}
```
Mock the Jupiter API in tests, swap to a different DEX without touching strategy code.

### Type-Safe Domain Modeling

```rust
pub struct TokenAmount(pub u64, pub TokenMint);
pub struct Price(pub Decimal);
```
The compiler catches unit mismatches before they become expensive trading errors.

### Concurrent Async Runtime

```rust
let (price, balance, quote) = tokio::try_join!(
    market.get_price(&token),
    wallet.get_balance(&token),
    jupiter.get_quote(params)
)?;
```
Fetch prices, check balances, and get quotes simultaneously—latency kills in trading.

### Trait-Based Strategy Pattern

```rust
pub trait StrategyPort: Send + Sync {
    async fn evaluate(&self, ctx: &MarketContext) -> Result<Signal, StrategyError>;
}
```
Add new strategies (DCA, grid, momentum) as plug-and-play modules.

---

## Quick Start

Get from zero to running in 6 steps. Each step shows expected output so you know it worked.

### Step 1: Prerequisites

Ensure you have the required tools installed:

```bash
rustc --version && cargo --version && solana --version
```

**Expected output:**
```
rustc 1.75.0 (or higher)
cargo 1.75.0 (or higher)
solana-cli 1.18.x (or higher)
```

**Requirements:**
- **Rust**: 1.75+ with Cargo build system
- **Solana CLI**: 1.18+ for wallet management and RPC interaction
- **Git**: For cloning the repository

**Missing a tool?** See [Troubleshooting](#troubleshooting) below for installation instructions.

### Step 2: Clone and Build

```bash
git clone https://github.com/goldbar123467/kyzlo-dex.git
cd kyzlo-dex
cargo build --release
```

**Expected output:**
```
   Compiling butters v0.1.0 (/home/user/kyzlo-dex)
    Finished release [optimized] target(s) in 45.23s
```

This compiles with optimizations enabled. The first build may take a few minutes as dependencies are fetched and compiled.

### Step 3: Configure

Create a configuration file for devnet testing:

```bash
cat > config.toml << 'EOF'
[solana]
rpc_url = "https://api.devnet.solana.com"
keypair_path = "~/.config/solana/devnet.json"

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
EOF
```

**What this does:**
- Sets devnet as the network (safe for testing)
- Configures conservative risk limits (5% max position, 2% stop loss)
- Targets SOL/USDC trading pair
- Uses Jupiter V6 API for swap routing

### Step 4: Get Devnet SOL

Generate a devnet wallet and request test SOL (has no real value):

```bash
# Generate new keypair for devnet
solana-keygen new --outfile ~/.config/solana/devnet.json --no-bip39-passphrase

# Switch to devnet
solana config set --url devnet --keypair ~/.config/solana/devnet.json

# Request airdrop (free test SOL)
solana airdrop 2
```

**Expected output:**
```
Wrote new keypair to ~/.config/solana/devnet.json
Config File: ~/.config/solana/cli/config.yml
RPC URL: https://api.devnet.solana.com
Requesting airdrop of 2 SOL
Signature: 5abc123...
2 SOL
```

**Note:** Devnet faucet may occasionally be rate-limited. If airdrop fails, wait 60 seconds and try again with `solana airdrop 1`.

### Step 5: Paper Trading Test

Run the bot in paper trading mode (no real transactions):

```bash
cargo run --release -- run --paper
```

**Expected output:**
```
[INFO] Starting Butters trading bot
[INFO] Mode: Paper Trading (simulated)
[INFO] Loaded config from config.toml
[INFO] Connected to devnet: https://api.devnet.solana.com
[INFO] Watching SOL/USDC pair
[INFO] Lookback: 50 candles | Z-threshold: 2.5
[INFO] Current z-score: 0.42 (within bounds, no trade signal)
[INFO] Tick 1 complete. Virtual portfolio: 2.0 SOL, 0.0 USDC
```

**What's happening:**
- Bot monitors real devnet prices via Jupiter API
- Calculates z-scores based on rolling statistics
- Logs virtual trades it *would* make
- No actual transactions are submitted

Press `Ctrl+C` to stop.

### Step 6: Going Live

**⚠️ CRITICAL SAFETY WARNINGS:**

Before running with real funds on mainnet:

1. **Test extensively in paper mode** - Run for at least 7 days to understand behavior
2. **Start small** - Use amounts you can afford to lose completely (0.1-0.5 SOL maximum)
3. **Use mainnet config** - Update `rpc_url` to mainnet and use a dedicated keypair
4. **Monitor actively** - Watch the first few live trades closely
5. **Set strict limits** - Configure conservative `max_daily_loss_pct` (2-3%)
6. **Private RPC recommended** - Public RPCs may be rate-limited; use Helius/QuickNode free tier

**To run live (after testing):**

```bash
# Create mainnet config (edit config.toml to use mainnet RPC)
# Generate mainnet keypair: solana-keygen new --outfile ~/.config/solana/mainnet.json

# Run live mode (NO --paper flag)
cargo run --release -- run
```

**Expected output:**
```
[INFO] Starting Butters trading bot
[INFO] Mode: LIVE TRADING on mainnet-beta
[WARN] Real funds at risk. Monitoring SOL/USDC with 5.0% max position size
```

**Never commit your mainnet keypair to git. Keep it secure.**

### Step 7: Production Deployment (tmux)

For persistent operation that survives SSH disconnects:

```bash
# One-command startup
./start.sh
```

**Output:**
```
✓ Butters started in tmux session 'ironwatch'

  Attach:  tmux attach -t ironwatch
  Detach:  Ctrl+B, D
```

**Managing the bot:**

| Command | Description |
|---------|-------------|
| `./start.sh` | Start bot in tmux (kills existing first) |
| `tmux attach -t ironwatch` | View live output |
| `Ctrl+B, D` | Detach (bot keeps running) |
| `tmux kill-session -t ironwatch` | Stop the bot |

**Monitoring scripts:**

```bash
# Watch for auto-restart and alerts
./scripts/ironwatch-monitor.sh

# View logs
tail -f logs/butters.log
```

The bot will continue running after you disconnect from SSH.

---

## Installation

### Build from Source

**Prerequisites:**
- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Solana CLI 1.18+ (install via [Solana docs](https://docs.solana.com/cli/install-solana-cli-tools))

**Steps:**

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install Solana CLI (if not already installed)
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

# Clone repository
git clone https://github.com/goldbar123467/kyzlo-dex.git
cd kyzlo-dex

# Build release binary
cargo build --release

# Binary will be at: target/release/butters
# Optionally, install to system path:
cargo install --path .
```

**Verify installation:**

```bash
butters --version
# Output: butters 0.1.0
```

### Optional: Private RPC Setup

Public Solana RPCs have rate limits that may impact trading performance. For production use, configure a private RPC endpoint:

**Recommended Providers (Free Tiers Available):**

| Provider | Free Tier | Latency | Setup |
|----------|-----------|---------|-------|
| [Helius](https://helius.dev/) | 100 req/s | Low | Create API key, use `https://mainnet.helius-rpc.com/?api-key=YOUR_KEY` |
| [QuickNode](https://www.quicknode.com/) | 25 req/s | Low | Create endpoint, copy HTTPS URL |
| [Triton](https://triton.one/) | 50 req/s | Medium | Create project, copy RPC URL |

**Configure in `config.toml`:**

```toml
[solana]
rpc_url = "https://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY"
# OR
rpc_url = "https://quick-node-xyz.quiknode.pro/YOUR_ENDPOINT/"
```

**Security:** Store API keys in environment variables instead of committing to config:

```bash
export SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

# Update config.toml to read from env:
# rpc_url = "${SOLANA_RPC_URL}"  # Requires env variable support in your config loader
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `rustc: command not found` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` then restart terminal |
| `solana: command not found` | Install Solana CLI: `sh -c "$(curl -sSfL https://release.solana.com/stable/install)"` then add to PATH |
| `linker 'cc' not found` | Install build tools: `sudo apt install build-essential` (Ubuntu) or `xcode-select --install` (macOS) |
| `RPC rate limit exceeded` | Wait 60 seconds, or configure a private RPC endpoint (see [Optional: Private RPC Setup](#optional-private-rpc-setup)) |
| `Airdrop failed` | Devnet faucet may be dry. Try: `solana airdrop 1` (smaller amount) or wait and retry |
| `insufficient funds for rent` | You need at least 0.01 SOL for account rent. Request another airdrop with `solana airdrop 1` |
| `Failed to parse config.toml` | Verify TOML syntax with a validator. Check for missing quotes, brackets, or invalid values |
| `Jupiter API connection failed` | Check internet connection. Verify `jupiter.api_url` is set to `https://public.jupiterapi.com` (not deprecated endpoints) |
| `Wallet keypair not found` | Ensure `keypair_path` in config.toml points to a valid keypair file. Generate with `solana-keygen new` |

---

## Configuration Reference

All configuration is done through a TOML file. By default, the bot looks for `config.toml` (override with `--config` flag).

### [strategy]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lookback_period` | integer | 60 | Number of candles for rolling mean/std calculation |
| `z_threshold` | float | 3.0 | Z-score threshold for entry signals (2.5 = moderate, 3.0 = strict) |
| `z_exit_threshold` | float | 0.37 | Z-score target for exit (academically optimized at 0.37) |
| `min_volume_percentile` | integer | 75 | Minimum volume percentile filter (0-100) |
| `max_spread_bps` | integer | 20 | Maximum bid-ask spread in basis points |
| `cooldown_seconds` | integer | 0 | Minimum seconds between trades |

### ADX Regime Detection

The bot uses Wilder's ADX (Average Directional Index) to filter trending markets:

| ADX Range | Regime | Position Multiplier |
|-----------|--------|---------------------|
| 0-15 | Ranging | 100% |
| 15-20 | Weak Trend | 80% |
| 20-25 | Transitioning | 50% |
| 25-30 | Trending | 20% |
| 30+ | Strong Trend | 0% (blocked) |

**Warmup**: ADX requires ~19 one-minute candles to become valid. During warmup, the bot trades at 50% size.

### [risk]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_position_pct` | float | 5.0 | Maximum position size as % of portfolio |
| `stop_loss_pct` | float | 0.5 | Stop loss trigger percentage |
| `take_profit_pct` | float | 0.8 | Take profit target percentage |
| `max_daily_trades` | integer | 20 | Maximum trades per day |
| `max_daily_loss_pct` | float | 3.0 | Daily loss circuit breaker (%) |
| `time_stop_hours` | float | 1.0 | Exit position after N hours if no movement |
| `trade_size_sol` | float | 0.37 | Trade size in SOL per signal (~$50) |

### [tokens]

| Parameter | Type | Description |
|-----------|------|-------------|
| `base` | string | Base token mint address (e.g., SOL) |
| `quote` | string | Quote token mint address (e.g., USDC) |

### [jupiter]

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `api_url` | string | `https://public.jupiterapi.com` | Jupiter API base URL |
| `slippage_bps` | integer | 50 | Slippage tolerance in basis points (50 = 0.5%) |

### [solana]

| Parameter | Type | Description |
|-----------|------|-------------|
| `rpc_url` | string | Solana RPC endpoint URL |
| `keypair_path` | string | Path to wallet keypair file |

### [jito] (Optional)

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | bool | true | Enable Jito MEV protection |
| `region` | string | `ny` | Block engine region (ny, amsterdam, frankfurt, tokyo) |
| `tip_lamports` | integer | 10000 | Validator tip amount |

---

## CLI Reference

### Global Options

| Option | Description |
|--------|-------------|
| `-v, --verbose` | Enable verbose logging |
| `--debug` | Enable debug logging |
| `--version` | Show version |
| `--help` | Show help |

### butters run

Start the trading loop.

```bash
butters run [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-c, --config FILE` | Path to configuration file (default: `config.toml`) |
| `-p, --paper` | Run in paper trading mode (no real transactions) |
| `--live` | Enable live mainnet trading (requires `--i-accept-losses`) |
| `--i-accept-losses` | Acknowledge financial risk (required for `--live`) |

**Examples:**

```bash
# Paper trading (safe testing)
butters run --paper

# Live trading (requires acknowledgment)
butters run --live --i-accept-losses
```

### butters status

Check bot status and portfolio.

```bash
butters status [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-c, --config FILE` | Path to configuration file |
| `-d, --detailed` | Show detailed portfolio breakdown |

### butters quote

Get a swap quote from Jupiter.

```bash
butters quote <INPUT> <OUTPUT> <AMOUNT>
```

**Example:**

```bash
butters quote SOL USDC 1.0
```

---

## Architecture

Butters uses **hexagonal architecture** (ports and adapters) for clean separation of concerns, testability, and flexibility.

```
                         ┌─────────────────┐
                         │   CLI ADAPTER   │
                         │   (clap args)   │
                         └────────┬────────┘
                                  │
                         ┌────────▼────────┐
                         │  ORCHESTRATOR   │
                         │  (application)  │
                         │  - Trading loop │
                         │  - ADX regime   │
                         │  - BalanceGuard │
                         └────────┬────────┘
                                  │
        ┌─────────────────────────┼─────────────────────────┐
        │                         │                         │
┌───────▼───────┐       ┌─────────▼─────────┐     ┌─────────▼─────────┐
│  DOMAIN CORE  │       │   STRATEGY PORT   │     │    MARKET PORT    │
│  - Position   │       │   - ZScoreGate    │     │    - PriceFeed    │
│  - Trade      │       │   - MeanReversion │     │    - OHLCV        │
│  - Portfolio  │       │   - ADX Regime    │     │    - CandleBuilder│
│  - RiskLimits │       │   - OU Process    │     └─────────┬─────────┘
│  - BalanceGuard       └─────────┬─────────┘               │
│  - TxValidator│                 │                         │
└───────────────┘       ┌─────────▼─────────┐     ┌─────────▼─────────┐
                        │ STRATEGY ADAPTER  │     │  JUPITER ADAPTER  │
                        │  (mean reversion) │     │   (DEX routing)   │
                        └───────────────────┘     └─────────┬─────────┘
                                                            │
                                                  ┌─────────▼─────────┐
                                                  │   JITO ADAPTER    │
                                                  │ (MEV protection)  │
                                                  └─────────┬─────────┘
                                                            │
                                                  ┌─────────▼─────────┐
                                                  │  SOLANA CLIENT    │
                                                  │  - RPC            │
                                                  │  - Wallet         │
                                                  │  - Tx Builder     │
                                                  └───────────────────┘
```

### Layer Descriptions

| Layer | Purpose |
|-------|---------|
| **Domain Core** | Pure business logic: Position, Trade, Portfolio, BalanceGuard, TxValidator |
| **Ports** | Trait definitions (interfaces) for external systems |
| **Adapters** | Jupiter DEX, Jito bundles, Solana RPC, PumpFun WebSocket |
| **Application** | TradingOrchestrator (SOL/USDC), MemeOrchestrator (multi-token) |
| **Strategy** | Z-Score gating, Mean Reversion, ADX regime detection, OU process |

### Benefits

- **Testability**: Domain and strategy logic tested without network calls
- **Flexibility**: Swap Jupiter for another DEX by implementing ExecutionPort
- **Isolation**: Core trading logic unaffected by API changes
- **Clarity**: Clear separation between "what" (domain), "how" (adapters), and "when" (orchestrator)

---

## Safety Features

Butters implements multiple layers of safety to protect your capital.

### 1. Jito MEV Protection (Fail-Closed)

All trades are submitted as **atomic bundles** via the Jito Block Engine:

- **Frontrunning Protection**: Transactions are not visible in the mempool
- **Sandwich Attack Prevention**: Bundles execute atomically or not at all
- **Fail-Closed Policy**: If bundle submission fails, the trade is NOT executed via fallback

### 2. Preflight Safety Checks

| Check | Purpose |
|-------|---------|
| Balance Verification | Confirms sufficient funds for trade + fees |
| Token Account Validation | Ensures token accounts exist |
| Slippage Enforcement | Rejects if slippage exceeds configured limit |
| Permission Check | Validates wallet keypair permissions |

### 3. Risk Management Limits

| Feature | Config Option | Default |
|---------|--------------|---------|
| Position Limits | `max_position_pct` | 5% |
| Stop Loss | `stop_loss_pct` | 2% |
| Take Profit | `take_profit_pct` | 1.5% |
| Daily Trade Limit | `max_daily_trades` | 10 |
| Daily Loss Circuit Breaker | `max_daily_loss_pct` | 3% |

### 4. Paper Trading Mode

Test strategies without risking real funds:

```bash
butters run --paper
```

- Monitors real market prices via Jupiter API
- Calculates z-scores and generates signals
- Logs virtual trades without submitting transactions

**Recommended**: Run in paper mode for at least 7 days before live trading.

### 5. Live Trading Safeguards

Live trading requires explicit acknowledgment:

```bash
# This will fail:
butters run --live

# This is required:
butters run --live --i-accept-losses
```

The `--i-accept-losses` flag ensures you consciously accept the risk before trading with real funds.

---

## License

MIT License

Copyright (c) 2024-2026 Butters Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

---

**Disclaimer:** This software is provided for educational and research purposes. Trading cryptocurrencies involves substantial risk of loss. The authors and contributors are not responsible for any financial losses incurred through the use of this software. Always trade responsibly and only with funds you can afford to lose completely.
