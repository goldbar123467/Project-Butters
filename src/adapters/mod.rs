//! Adapters Layer - External System Implementations
//!
//! This module contains implementations of the port traits:
//! - Jupiter: DEX aggregator API client, token list, and prices
//! - Solana: RPC client and wallet management
//! - CLI: Command-line interface handlers
//! - Market Data: Price feeds and market data
//! - Jito: MEV-protected bundle submission
//! - Token Metadata: Mint authority, freeze authority, supply info
//! - Pump.fun: Real-time token launch monitoring via WebSocket
//! - Honeypot: Token-2022 extension analysis and sell simulation

pub mod jupiter;
pub mod solana;
pub mod cli;
pub mod market_data;
pub mod jito;
pub mod token_metadata;
pub mod pump_fun;
pub mod honeypot;

