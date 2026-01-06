//! Strategy Layer - Mean Reversion with Z-Score Gating
//!
//! Implements a conservative mean reversion strategy:
//! - Z-score threshold of 2.5 for entry signals
//! - Rolling statistics over configurable lookback period
//! - Volume and spread filters for noise reduction

pub mod params;
pub mod zscore_gate;
pub mod mean_reversion;

pub use params::{StrategyConfig, RiskConfig, FilterConfig};
pub use zscore_gate::{ZScoreGate, ZScoreResult};
pub use mean_reversion::{MeanReversionStrategy, TradeAction, PositionState};
