//! Market Data Adapters
//!
//! External data sources for price feeds and token discovery:
//! - `JupiterPriceClient`: Jupiter price API client for real-time prices
//! - `TokenScanner`: Jupiter API-based token discovery for meme coins
//!
//! Token Scanner Features:
//! - Rate limiting (60 RPM for Jupiter free tier)
//! - Batch price fetching for multiple tokens
//! - Configurable filters (volume, liquidity, spread, age)
//! - Automatic filtering of stablecoins and wrapped tokens

mod jupiter_price;
mod token_scanner;

pub use jupiter_price::JupiterPriceClient;
pub use token_scanner::{TokenScanner, ScannerConfig, MemeToken, ScannerError, RateLimiter};
