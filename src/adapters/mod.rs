//! Adapters Layer - External System Implementations
//!
//! This module contains implementations of the port traits:
//! - Jupiter: DEX aggregator API client
//! - Solana: RPC client and wallet management
//! - CLI: Command-line interface handlers
//! - Market Data: Price feeds, token discovery, and market data
//! - Jito: MEV-protected bundle submission
//! - Pump.fun: Bonding curve price fetching for pre-graduation tokens

pub mod jupiter;
pub mod solana;
pub mod cli;
pub mod market_data;
pub mod jito;
pub mod pump_fun;

pub use jupiter::JupiterClient;
pub use solana::{SolanaClient, WalletManager};
pub use cli::CliApp;
pub use market_data::{JupiterPriceClient, TokenScanner, ScannerConfig, MemeToken};
pub use jito::JitoBundleClient;
pub use pump_fun::{BondingCurveState, PumpFunTokenState};
