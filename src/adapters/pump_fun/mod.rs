//! Pump.fun Adapter
//!
//! This module provides integration with pump.fun, the most popular
//! meme coin launchpad on Solana. Pump.fun tokens trade on a bonding
//! curve until they reach graduation (~85 SOL / ~$69K market cap),
//! at which point they migrate to Raydium.
//!
//! Key features:
//! - Bonding curve price calculation
//! - Pre-graduation price fetching (no Jupiter needed)
//! - Graduation status tracking
//!
//! For tokens still on the bonding curve, Jupiter quotes will fail
//! with "Could not find any route". Use the bonding curve price instead.

pub mod types;

pub use types::{
    BondingCurveState, PumpFunTokenState, BONDING_CURVE_TOKENS, GRADUATION_SOL_THRESHOLD,
    INITIAL_VIRTUAL_SOL, INITIAL_VIRTUAL_TOKENS, PUMP_TOKEN_DECIMALS, PUMP_TOTAL_SUPPLY,
    SOL_DECIMALS,
};
