use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Core trait for strategy implementations
pub trait StrategyPort {
    /// Generate trading signals based on current market state
    fn generate_signals(&mut self, data: &[f64]) -> Result<Vec<Signal>, StrategyError>;
    
    /// Calculate technical indicators used by the strategy
    fn calculate_indicators(&mut self, data: &[f64]) -> Result<IndicatorValues, StrategyError>;
    
    /// Validate strategy parameters
    fn validate_params(&self) -> Result<(), StrategyError>;
}

/// Trading signal types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
    StrongBuy,
    StrongSell,
}

/// Container for indicator values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorValues {
    pub rsi: Option<f64>,
    pub macd: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub sma: Option<f64>,
    pub ema: Option<f64>,
}

/// Common strategy parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub rsi: Option<RsiParams>,
    pub macd: Option<MacdParams>,
    pub moving_avg: Option<MovingAvgParams>,
}

/// RSI-specific parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
}

/// MACD-specific parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdParams {
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
}

/// Moving average parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovingAvgParams {
    pub period: usize,
    pub ma_type: MovingAverageType,
}

/// Moving average calculation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MovingAverageType {
    Simple,
    Exponential,
    Smoothed,
}

/// Strategy validation errors
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum StrategyError {
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    
    #[error("Insufficient data for calculation: requires {0} elements, got {1}")]
    InsufficientData(usize, usize),
    
    #[error("Indicator calculation failed: {0}")]
    CalculationError(String),
    
    #[error("Strategy configuration error: {0}")]
    ConfigurationError(String),
}