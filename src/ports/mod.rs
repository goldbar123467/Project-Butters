//! Ports Layer - Trait definitions for external dependencies
//!
//! This module defines the interfaces (ports) that adapters must implement.
//! Following hexagonal architecture, these traits abstract:
//! - Market data feeds (prices, OHLCV)
//! - Trade execution (Jupiter swaps)
//! - Strategy signal generation

pub mod market_data;
pub mod execution;
pub mod strategy;
pub mod models;

// Re-export main traits and types
