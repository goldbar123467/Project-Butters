//! Market Regime Detection Module
//!
//! Provides trait-based regime detection to identify when market conditions
//! are favorable for trading strategies.
//!
//! - **ADX Regime Detector**: For mean reversion (low ADX = favorable)
//! - **Momentum ADX Detector**: For momentum trading (high ADX = favorable)
//! - **Candle Builder**: Builds OHLC candles from price ticks

pub mod adx;
mod candle_builder;
pub mod momentum_adx;

pub use adx::{AdxRegimeDetector, AdxConfig, AdxResult, TrendRegime, TrendDirection};
pub use candle_builder::CandleBuilder;
pub use momentum_adx::{MomentumAdxDetector, MomentumAdxConfig, MomentumSignal};

/// OHLC candle data for regime detection
#[derive(Debug, Clone, Copy)]
pub struct Candle {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    pub fn new(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self { open, high, low, close, volume }
    }

    /// Validate OHLC data integrity
    pub fn is_valid(&self) -> bool {
        self.high >= self.low
            && self.close >= self.low
            && self.close <= self.high
            && self.open >= self.low
            && self.open <= self.high
            && self.high.is_finite()
            && self.low.is_finite()
            && self.close.is_finite()
            && self.open.is_finite()
    }
}

/// Signal from a regime detector with confidence level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegimeSignal {
    /// Conditions favorable for mean reversion (confidence 0.7-1.0)
    Favorable(f64),
    /// Neutral conditions, proceed with caution (confidence 0.4-0.7)
    Neutral(f64),
    /// Unfavorable conditions, avoid mean reversion (confidence 0.0-0.4)
    Unfavorable(f64),
}

impl RegimeSignal {
    /// Get the confidence value
    pub fn confidence(&self) -> f64 {
        match self {
            Self::Favorable(c) | Self::Neutral(c) | Self::Unfavorable(c) => *c,
        }
    }

    /// Create signal from confidence value
    pub fn from_confidence(c: f64) -> Self {
        let clamped = c.clamp(0.0, 1.0);
        if clamped >= 0.7 {
            Self::Favorable(clamped)
        } else if clamped >= 0.4 {
            Self::Neutral(clamped)
        } else {
            Self::Unfavorable(clamped)
        }
    }

    /// Check if trading should be allowed
    pub fn allows_trading(&self) -> bool {
        matches!(self, Self::Favorable(_) | Self::Neutral(_))
    }

    /// Get position size multiplier based on signal
    pub fn position_multiplier(&self) -> f64 {
        match self {
            Self::Favorable(c) => 1.0 * c,
            Self::Neutral(c) => 0.5 * c,
            Self::Unfavorable(_) => 0.0,
        }
    }
}

/// Trait for market regime detection
///
/// Implementations process OHLC candle data to determine if market conditions
/// are favorable for the trading strategy.
pub trait RegimeDetector: Send + Sync {
    /// Update the detector with a new candle and get regime signal
    fn update(&mut self, candle: &Candle) -> Option<RegimeSignal>;

    /// Get the recommended position size multiplier (0.0 to 1.0)
    fn get_position_multiplier(&self) -> f64;

    /// Check if detector has processed enough data to be reliable
    fn is_ready(&self) -> bool;

    /// Get detector name for logging/display
    fn name(&self) -> &'static str;

    /// Reset detector state
    fn reset(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candle_validation() {
        let valid = Candle::new(100.0, 105.0, 95.0, 102.0, 1000.0);
        assert!(valid.is_valid());

        // Invalid: high < low
        let invalid = Candle::new(100.0, 95.0, 105.0, 100.0, 1000.0);
        assert!(!invalid.is_valid());

        // Invalid: close outside range
        let invalid2 = Candle::new(100.0, 105.0, 95.0, 110.0, 1000.0);
        assert!(!invalid2.is_valid());
    }

    #[test]
    fn test_regime_signal_from_confidence() {
        assert!(matches!(RegimeSignal::from_confidence(0.9), RegimeSignal::Favorable(_)));
        assert!(matches!(RegimeSignal::from_confidence(0.5), RegimeSignal::Neutral(_)));
        assert!(matches!(RegimeSignal::from_confidence(0.2), RegimeSignal::Unfavorable(_)));
    }

    #[test]
    fn test_regime_signal_allows_trading() {
        assert!(RegimeSignal::Favorable(0.9).allows_trading());
        assert!(RegimeSignal::Neutral(0.5).allows_trading());
        assert!(!RegimeSignal::Unfavorable(0.2).allows_trading());
    }

    #[test]
    fn test_position_multiplier() {
        let favorable = RegimeSignal::Favorable(0.9);
        assert!((favorable.position_multiplier() - 0.9).abs() < 0.001);

        let neutral = RegimeSignal::Neutral(0.6);
        assert!((neutral.position_multiplier() - 0.3).abs() < 0.001);

        let unfavorable = RegimeSignal::Unfavorable(0.2);
        assert_eq!(unfavorable.position_multiplier(), 0.0);
    }

    #[test]
    fn test_confidence_clamping() {
        let over = RegimeSignal::from_confidence(1.5);
        assert_eq!(over.confidence(), 1.0);

        let under = RegimeSignal::from_confidence(-0.5);
        assert_eq!(under.confidence(), 0.0);
    }
}
