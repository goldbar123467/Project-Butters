//! Pump.fun Adapter
//!
//! Real-time monitoring of pump.fun token launches via WebSocket.
//! Provides detection of new meme coin launches and trade activity.
//!
//! # Overview
//!
//! Pump.fun is a Solana-based platform for launching meme coins with bonding curves.
//! This adapter connects to their WebSocket API to receive real-time updates on:
//! - New token creations
//! - Trade activity on specific tokens
//! - Account activity (for tracking specific wallets)
//!
//! # Example
//!
//! ```ignore
//! use butters::adapters::pump_fun::{PumpFunMonitor, PumpMonitorBuilder, PumpEvent};
//!
//! // Create monitor with builder
//! let (monitor, mut rx) = PumpMonitorBuilder::new()
//!     .subscribe_new_tokens(true)
//!     .auto_reconnect(true)
//!     .build();
//!
//! // Spawn monitor task
//! tokio::spawn(async move {
//!     monitor.run().await.expect("Monitor failed");
//! });
//!
//! // Process events
//! while let Some(event) = rx.recv().await {
//!     match event {
//!         PumpEvent::NewToken { mint, name, symbol, market_cap_sol, .. } => {
//!             println!("New token: {} ({}) - mcap: {} SOL", name, symbol, market_cap_sol);
//!         }
//!         PumpEvent::TokenTrade { mint, is_buy, sol_amount, .. } => {
//!             let action = if is_buy { "BUY" } else { "SELL" };
//!             println!("{} on {}: {} lamports", action, mint, sol_amount);
//!         }
//!         PumpEvent::GraduationProgress { mint, bonding_curve_percent, .. } => {
//!             println!("{} graduation progress: {:.1}%", mint, bonding_curve_percent);
//!         }
//!         _ => {}
//!     }
//! }
//! ```
//!
//! # WebSocket Protocol
//!
//! The pump.fun WebSocket API at `wss://pumpportal.fun/api/data` supports:
//!
//! - `subscribeNewToken` - Receive all new token creation events
//! - `subscribeTokenTrade` - Receive trade events for specific token mints
//! - `subscribeAccountTrade` - Receive trade events for specific wallet addresses
//!
//! Messages are JSON formatted with the subscription method and optional keys array.

mod monitor;
mod types;

pub use monitor::{
    PumpEvent, PumpFunMonitor, PumpMonitorBuilder, PumpMonitorConfig,
};
pub use types::{
    BondingCurveState, PumpFunToken, TradeInfo,
};
