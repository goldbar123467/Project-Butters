# Butters

[![Build](https://img.shields.io/badge/build-passing-brightgreen?style=flat-square)]()
[![Tests](https://img.shields.io/badge/tests-560%20passing-brightgreen?style=flat-square)]()
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/solana-mainnet-blueviolet?style=flat-square)](https://solana.com/)
[![Jupiter](https://img.shields.io/badge/jupiter-v1-blue?style=flat-square)](https://jup.ag/)
[![Jito](https://img.shields.io/badge/jito-MEV%20protected-purple?style=flat-square)](https://www.jito.wtf/)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)

**A modular Solana trading bot platform with multiple strategies, MEV protection, and comprehensive safety systems.**

---

## Trading Bots

Butters hosts multiple specialized trading bots on a shared infrastructure:

| Bot | Strategy | Target | Status |
|-----|----------|--------|--------|
| **Mean Reversion** | Z-Score + ADX regime filtering | SOL/USDC pair | Production |
| **Meme Sniper** | OU-GBM momentum + safety guards | Multi-token meme coins | Production |

### Mean Reversion Bot

Conservative statistical arbitrage for the SOL/USDC pair:
- Z-score entry at ±3.0 standard deviations
- ADX regime detection filters trending markets
- Targets extreme moves with 65-75% reversion probability

```bash
butters run --config config.toml --paper
```

### Meme Sniper Bot

Aggressive momentum trading for meme coins with comprehensive safety:
- OU-GBM (Ornstein-Uhlenbeck with Geometric Brownian Motion) signal generation
- Multi-token tracking with position management
- **Honeypot detection** - Token-2022 extensions, sell simulation
- **Rug detection** - Holder concentration, LP analysis
- **Liquidity guards** - Minimum thresholds for entry

```bash
butters meme run --config config.toml --paper
```

---

## Safety Stack

Both bots share a comprehensive safety infrastructure:

| Layer | Component | Protection |
|-------|-----------|------------|
| 1 | **HoneypotDetector** | Token-2022 extensions, unknown programs, sell simulation via Jupiter |
| 2 | **RugDetector** | Holder concentration, LP rug risk, metadata analysis |
| 3 | **LiquidityGuard** | Minimum liquidity thresholds, trend monitoring |
| 4 | **BalanceGuard** | Pre/post trade balance validation, anomaly detection |
| 5 | **TxValidator** | Program whitelist, instruction validation |
| 6 | **Jito MEV Protection** | Atomic bundles, fail-closed policy |

### Honeypot Detection (New)

The meme bot includes production honeypot detection:

```
Token Analysis Flow:
1. Validate token program (SPL/Token-2022, block unknown)
2. Parse authorities (mint/freeze) from raw account data
3. Detect Token-2022 extensions:
   - PermanentDelegate → BLOCK (can steal tokens)
   - NonTransferable → BLOCK (cannot sell)
   - TransferHook → Check whitelist (Metaplex OK, unknown BLOCK)
   - TransferFee > 25% → BLOCK
4. Simulate sell via Jupiter quote
5. Cache result (safe=5min, honeypot=1hr)
```

---

## Features

| Feature | Description |
|---------|-------------|
| **Multi-Bot Platform** | Run mean reversion or meme strategies from the same binary |
| **Z-Score Statistical Gating** | Trades when price deviates significantly from rolling mean |
| **ADX Regime Detection** | Filters trending markets where mean reversion fails |
| **OU-GBM Signal Generation** | Momentum detection for meme coin trading |
| **Honeypot Detection** | Token-2022 extension analysis and sell simulation |
| **Rug Pull Detection** | Holder concentration and LP monitoring |
| **Hexagonal Architecture** | Clean separation of domain, ports, and adapters |
| **Paper Trading Mode** | Simulate trades without risking real funds |
| **Jupiter V1 Integration** | Access to 20+ Solana DEXs with smart routing |
| **Jito MEV Protection** | Atomic bundles prevent frontrunning and sandwich attacks |
| **BalanceGuard Security** | Pre/post trade validation detects unexpected losses |
| **Position Persistence** | Crash recovery with state persistence to disk |

---

## Quick Start

### Prerequisites

```bash
rustc --version && cargo --version && solana --version
# Requires: Rust 1.75+, Solana CLI 1.18+
```

### Build

```bash
git clone https://github.com/goldbar123467/kyzlo-dex.git
cd kyzlo-dex
cargo build --release
```

### Run Mean Reversion Bot (SOL/USDC)

```bash
# Paper trading (safe)
./target/release/butters run --config config.toml --paper

# Live trading (requires acknowledgment)
./target/release/butters run --config config.toml --live --i-accept-losses
```

### Run Meme Sniper Bot

```bash
# Paper trading with specific tokens
./target/release/butters meme run -c config.toml --paper --tokens "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"

# Live trading
./target/release/butters meme run -c config.toml --live --i-accept-losses
```

---

## Architecture

```
                              ┌──────────────────────┐
                              │      CLI ADAPTER     │
                              │   butters run/meme   │
                              └──────────┬───────────┘
                                         │
              ┌──────────────────────────┼──────────────────────────┐
              │                          │                          │
   ┌──────────▼──────────┐    ┌──────────▼──────────┐    ┌──────────▼──────────┐
   │  TradingOrchestrator │    │  MemeOrchestrator   │    │   Shared Domain     │
   │    (SOL/USDC)        │    │  (Multi-token)      │    │                     │
   │  - Z-Score strategy  │    │  - OU-GBM strategy  │    │  - BalanceGuard     │
   │  - ADX regime        │    │  - Token tracking   │    │  - TxValidator      │
   │  - Single pair       │    │  - Position mgmt    │    │  - Position/Trade   │
   └──────────┬───────────┘    └──────────┬──────────┘    │  - Portfolio        │
              │                           │               └─────────────────────┘
              │                           │
              │              ┌────────────┴────────────┐
              │              │                         │
              │   ┌──────────▼──────────┐   ┌─────────▼─────────┐
              │   │  HoneypotDetector   │   │    RugDetector    │
              │   │  - Token-2022 ext   │   │  - Holder conc    │
              │   │  - Sell simulation  │   │  - LP analysis    │
              │   │  - Hook whitelist   │   │  - Metadata       │
              │   └─────────────────────┘   └───────────────────┘
              │
   ┌──────────┴───────────────────────────────────────────────┐
   │                      ADAPTERS                             │
   │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
   │  │ Jupiter │  │  Jito   │  │ Solana  │  │  Pump.fun   │  │
   │  │  (DEX)  │  │  (MEV)  │  │  (RPC)  │  │ (WebSocket) │  │
   │  └─────────┘  └─────────┘  └─────────┘  └─────────────┘  │
   └───────────────────────────────────────────────────────────┘
```

### Layer Descriptions

| Layer | Purpose |
|-------|---------|
| **Domain Core** | Pure business logic: Position, Trade, Portfolio, BalanceGuard, TxValidator |
| **Safety Modules** | HoneypotDetector, RugDetector, LiquidityGuard |
| **Orchestrators** | TradingOrchestrator (SOL/USDC), MemeOrchestrator (multi-token) |
| **Strategies** | Z-Score gating, Mean Reversion, ADX regime, OU-GBM process |
| **Adapters** | Jupiter DEX, Jito bundles, Solana RPC, Pump.fun WebSocket |

---

## CLI Reference

### Global Options

| Option | Description |
|--------|-------------|
| `-v, --verbose` | Enable verbose logging |
| `--debug` | Enable debug logging |
| `--version` | Show version |
| `--help` | Show help |

### Mean Reversion Bot: `butters run`

```bash
butters run [OPTIONS]

Options:
  -c, --config <FILE>     Configuration file [default: config.toml]
  -p, --paper             Paper trading mode
      --live              Live trading mode
      --i-accept-losses   Acknowledge financial risk
```

### Meme Sniper Bot: `butters meme`

```bash
butters meme <COMMAND>

Commands:
  run           Start meme trading loop
  status        Show orchestrator status
  add-token     Add token to tracking
  remove-token  Remove token from tracking
  list-tokens   List tracked tokens
  position      Show active position
  exit          Force exit position
```

#### `butters meme run`

```bash
butters meme run [OPTIONS]

Options:
  -c, --config <FILE>         Configuration file [default: config/mainnet.toml]
  -p, --paper                 Paper trading mode
      --live                  Live trading mode
      --i-accept-losses       Acknowledge financial risk
      --tokens <MINTS>        Comma-separated token mints to track
      --trade-size <USDC>     Trade size in USDC [default: 50.0]
      --poll-interval <SECS>  Price poll interval [default: 60]
```

---

## Configuration

### Meme Bot Settings

```toml
[meme]
trade_size_usdc = 50.0
poll_interval_secs = 60
ou_lookback = 20
ou_dt_minutes = 1.0
z_entry_threshold = -3.5
z_exit_threshold = 0.0
stop_loss_pct = 10.0
take_profit_pct = 15.0
momentum_enabled = true
min_hold_minutes = 5
```

### Safety Settings

```toml
[honeypot]
reject_transfer_hooks = true
reject_permanent_delegate = true
reject_freeze_authority = false
max_transfer_fee_bps = 2500
require_sell_simulation = true

[rug]
max_holder_concentration = 0.50
min_liquidity_usd = 10000
```

---

## Performance

| Metric | Value |
|--------|-------|
| Trade Execution | ~3 seconds (signal to confirmation) |
| Honeypot Analysis | <100ms (cached: <1ms) |
| Quote Latency | <20ms from Jupiter API |
| Binary Size | ~10MB single executable |
| Memory Footprint | <50MB runtime |
| Test Suite | 560+ tests in <10 seconds |

---

## Test Results

```bash
cargo test --lib
# 560 passed, 1 ignored, 0 failed
```

| Module | Tests |
|--------|-------|
| Domain (core logic) | 89 |
| Honeypot Detection | 49 |
| Strategy (Z-Score, OU-GBM) | 78 |
| Adapters (Jupiter, Jito) | 112 |
| Orchestrators | 45 |
| Other | 187 |

---

## Safety Features

### 1. Jito MEV Protection (Fail-Closed)

All trades submitted as atomic bundles via Jito Block Engine:
- Transactions not visible in mempool
- Bundles execute atomically or not at all
- If bundle submission fails, trade is NOT executed via fallback

### 2. Honeypot Detection

Before every meme trade entry:
- Validates token program (blocks unknown programs)
- Parses Token-2022 extensions for dangerous patterns
- Simulates sell via Jupiter quote
- Caches results with asymmetric TTL

### 3. Balance Guard

Pre/post trade validation:
- Captures expected balance delta before trade
- Validates actual delta matches expected after trade
- Halts trading on anomalies

### 4. Paper Trading Mode

Test strategies without risking real funds:
```bash
butters run --paper
butters meme run --paper
```

---

## License

MIT License - See [LICENSE](LICENSE) for details.

---

**Disclaimer:** This software is provided for educational and research purposes. Trading cryptocurrencies involves substantial risk of loss. The authors and contributors are not responsible for any financial losses incurred through the use of this software. Always trade responsibly and only with funds you can afford to lose completely.
