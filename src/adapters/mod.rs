//! Adapters Layer - External System Implementations
//!
//! This module contains implementations of the port traits:
//! - Jupiter: DEX aggregator API client
//! - Solana: RPC client and wallet management
//! - CLI: Command-line interface handlers
//! - Market Data: Price feeds and market data

pub mod jupiter;
pub mod solana;
pub mod cli;
pub mod market_data;

pub use jupiter::JupiterClient;
pub use solana::{SolanaClient, WalletManager};
pub use cli::CliApp;
pub use market_data::JupiterPriceClient;
