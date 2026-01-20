//! Strategy Layer - Mean Reversion with Z-Score Gating
//!
//! Implements a conservative mean reversion strategy:
//! - Z-score threshold of 2.5 for entry signals
//! - Rolling statistics over configurable lookback period
//! - Volume and spread filters for noise reduction
//! - ADX-based regime detection to filter trending markets
//! - OU process parameter estimation for meme coin mean reversion

pub mod params;
pub mod zscore_gate;
pub mod mean_reversion;
pub mod regime;
pub mod ou_process;
pub mod launch_sniper;

pub use params::StrategyConfig;
pub use mean_reversion::{MeanReversionStrategy, TradeAction, PositionState};
pub use regime::{
    RegimeDetector,
    AdxRegimeDetector, AdxConfig,
    CandleBuilder,
};
