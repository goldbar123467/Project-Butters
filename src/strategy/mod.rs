//! Strategy Layer - Mean Reversion with Z-Score Gating and Statistical Modeling
//!
//! Implements conservative mean reversion strategies with statistical enhancements:
//! - Z-score threshold gating for entry signals
//! - Rolling statistics over configurable lookback period
//! - Volume and spread filters for noise reduction
//! - OU (Ornstein-Uhlenbeck) process for mean reversion dynamics
//! - GBM (Geometric Brownian Motion) for drift and volatility estimation
//! - Combined OU-GBM strategy for high-conviction trades
//!
//! Strategy Options:
//! - `MeanReversionStrategy`: Basic z-score mean reversion
//! - `OUGBMStrategy`: Combined OU-GBM with drift alignment filter (RECOMMENDED)

pub mod params;
pub mod zscore_gate;
pub mod mean_reversion;
pub mod ou_process;
pub mod gbm_estimator;
pub mod ou_gbm_strategy;

pub use params::{StrategyConfig, RiskConfig, FilterConfig};
pub use zscore_gate::{ZScoreGate, ZScoreResult};
pub use mean_reversion::{MeanReversionStrategy, TradeAction, PositionState};
pub use ou_process::{OUProcess, OUParams, OUSignal};
pub use gbm_estimator::{GBMEstimator, GBMParams, DriftDirection};
pub use ou_gbm_strategy::{OUGBMStrategy, OUGBMConfig, OUGBMAction, OUGBMPositionState, OUGBMDiagnostics};
