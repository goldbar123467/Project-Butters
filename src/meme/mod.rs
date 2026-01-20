//! Meme Coin Trading Module
//!
//! Multi-token meme coin trading using OU-GBM mean reversion strategy.
//! This module provides:
//! - Configuration structures for meme trading parameters
//! - Type definitions for tokens, positions, and signals
//! - Traits for launch detection and token analysis
//! - CLI command handlers for meme trading operations
//!
//! # Architecture
//!
//! The meme module follows the hexagonal architecture pattern:
//!
//! ```text
//!                      CLI Commands
//!                           |
//!                           v
//!                    MemeOrchestrator (application layer)
//!                     /            \
//!                    v              v
//!            LaunchDetector    TokenAnalyzer
//!            (trait)            (trait)
//!                    \              /
//!                     v            v
//!                   Adapters (implementations)
//! ```
//!
//! # Configuration
//!
//! The `[meme]` section in config.toml controls meme trading behavior:
//!
//! ```toml
//! [meme]
//! enabled = true
//! trade_size_usdc = 50.0
//! z_entry_threshold = -3.5
//! # ... see MemeConfig for all options
//! ```
//!
//! # Usage
//!
//! ```text
//! butters meme run --paper              # Start paper trading
//! butters meme status                   # Show orchestrator status
//! butters meme add-token <MINT>         # Add token to tracking
//! butters meme list-tokens              # List tracked tokens
//! butters meme position                 # Show active position
//! butters meme exit --yes               # Force exit position
//! ```

pub mod commands;
pub mod config;
pub mod paper_trading;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use commands::execute_meme_command;
pub use config::{MemeConfig, MemeConfigError};
