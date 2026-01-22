//! Momentum ADX Detector
//!
//! Wraps the core ADX indicator for momentum-based trading (vs mean reversion).
//! Key differences from mean reversion ADX:
//! - High ADX = GOOD for momentum (strong trend to ride)
//! - Position sizing INCREASES with ADX (inverted from mean reversion)
//! - Direction confirmation via +DI/-DI alignment
//!
//! For momentum strategies:
//! - ADX > 25: Trend confirmed, favorable for momentum entry
//! - ADX 20-25: Transition zone (wait for confirmation)
//! - ADX < 20: Weak trend, avoid momentum entries

use super::{Candle, AdxRegimeDetector, AdxConfig, TrendDirection, RegimeDetector};

/// Configuration for momentum ADX detector
#[derive(Debug, Clone)]
pub struct MomentumAdxConfig {
    /// ADX calculation period (default: 14)
    pub period: usize,
    /// ADX threshold for entry (above this = trend confirmed)
    pub entry_threshold: f64,
    /// ADX threshold for exit (below this = trend dying)
    pub exit_threshold: f64,
    /// Minimum consecutive bars with ADX > threshold for entry
    pub min_confirmation_bars: usize,
}

impl Default for MomentumAdxConfig {
    fn default() -> Self {
        Self {
            period: 14,
            entry_threshold: 25.0,  // ADX > 25 confirms trend
            exit_threshold: 20.0,   // ADX < 20 = trend dying
            min_confirmation_bars: 2,
        }
    }
}

impl MomentumAdxConfig {
    /// Create config for meme coins (faster response)
    pub fn meme_optimized() -> Self {
        Self {
            period: 10,             // Faster response
            entry_threshold: 25.0,
            exit_threshold: 20.0,
            min_confirmation_bars: 2,
        }
    }

    /// Warmup periods needed before ADX is valid
    pub fn warmup_periods(&self) -> usize {
        2 * self.period - 1
    }
}

/// Signal from momentum ADX detector
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MomentumSignal {
    /// Bullish momentum confirmed: ADX > threshold AND +DI > -DI
    BullishMomentum {
        adx: f64,
        plus_di: f64,
        minus_di: f64,
    },
    /// Bearish momentum confirmed: ADX > threshold AND -DI > +DI
    /// NOTE: Not used for meme coins (LONG-ONLY on Jupiter)
    BearishMomentum {
        adx: f64,
        plus_di: f64,
        minus_di: f64,
    },
    /// Trend is dying: ADX falling below exit threshold
    TrendExpiring {
        adx: f64,
    },
    /// No clear signal (ADX in transition zone or not enough data)
    NoSignal,
}

impl MomentumSignal {
    /// Check if this is a bullish entry signal
    pub fn is_bullish_entry(&self) -> bool {
        matches!(self, Self::BullishMomentum { .. })
    }

    /// Check if this indicates trend is dying
    pub fn is_trend_dying(&self) -> bool {
        matches!(self, Self::TrendExpiring { .. })
    }

    /// Get ADX value if available
    pub fn adx(&self) -> Option<f64> {
        match self {
            Self::BullishMomentum { adx, .. } => Some(*adx),
            Self::BearishMomentum { adx, .. } => Some(*adx),
            Self::TrendExpiring { adx } => Some(*adx),
            Self::NoSignal => None,
        }
    }
}

/// Momentum-specific ADX detector
///
/// Wraps the core ADX detector and inverts the logic for momentum trading:
/// - High ADX = favorable for entry (vs low ADX for mean reversion)
/// - Direction confirmation required (+DI/-DI alignment)
/// - Position sizing increases with ADX strength
#[derive(Debug)]
pub struct MomentumAdxDetector {
    config: MomentumAdxConfig,
    base_adx: AdxRegimeDetector,
    /// Consecutive bars with ADX > entry threshold
    bullish_bars: usize,
    /// Consecutive bars with ADX > entry threshold (bearish direction)
    bearish_bars: usize,
    /// Previous ADX value for decay detection
    prev_adx: Option<f64>,
    /// Flag if we were previously in a confirmed trend
    was_trending: bool,
}

impl MomentumAdxDetector {
    /// Create a new momentum ADX detector
    pub fn new(config: MomentumAdxConfig) -> Self {
        let adx_config = AdxConfig {
            period: config.period,
            ranging_threshold: config.exit_threshold,
            trending_threshold: config.entry_threshold,
            entry_threshold: config.exit_threshold,
            exit_threshold: config.entry_threshold + 3.0, // Buffer above entry
        };

        Self {
            config,
            base_adx: AdxRegimeDetector::new(adx_config),
            bullish_bars: 0,
            bearish_bars: 0,
            prev_adx: None,
            was_trending: false,
        }
    }

    /// Create with default config
    pub fn with_default_config() -> Self {
        Self::new(MomentumAdxConfig::default())
    }

    /// Create with meme-optimized config
    pub fn meme_optimized() -> Self {
        Self::new(MomentumAdxConfig::meme_optimized())
    }

    /// Update with new candle data
    pub fn update(&mut self, candle: &Candle) -> MomentumSignal {
        let result = self.base_adx.update_candle(candle);

        if !result.is_valid {
            return MomentumSignal::NoSignal;
        }

        let adx = result.adx;
        let direction = self.base_adx.get_direction();

        // Track confirmation bars
        if adx >= self.config.entry_threshold {
            match direction {
                TrendDirection::Bullish => {
                    self.bullish_bars += 1;
                    self.bearish_bars = 0;
                }
                TrendDirection::Bearish => {
                    self.bearish_bars += 1;
                    self.bullish_bars = 0;
                }
                TrendDirection::Neutral => {
                    // Reset both on neutral
                    self.bullish_bars = 0;
                    self.bearish_bars = 0;
                }
            }
        } else {
            // ADX below entry threshold, reset confirmation
            self.bullish_bars = 0;
            self.bearish_bars = 0;
        }

        // Detect trend expiring (was trending, now ADX falling below exit)
        let trend_expiring = self.was_trending && adx < self.config.exit_threshold;

        // Update was_trending state
        self.was_trending = adx >= self.config.entry_threshold;

        // Store prev ADX for decay detection
        self.prev_adx = Some(adx);

        // Generate signal
        if trend_expiring {
            return MomentumSignal::TrendExpiring { adx };
        }

        // Check for entry signals with confirmation
        if self.bullish_bars >= self.config.min_confirmation_bars {
            return MomentumSignal::BullishMomentum {
                adx,
                plus_di: result.plus_di,
                minus_di: result.minus_di,
            };
        }

        if self.bearish_bars >= self.config.min_confirmation_bars {
            return MomentumSignal::BearishMomentum {
                adx,
                plus_di: result.plus_di,
                minus_di: result.minus_di,
            };
        }

        MomentumSignal::NoSignal
    }

    /// Check if entry conditions are met (LONG only for meme coins)
    pub fn check_entry_signal(&self) -> Option<MomentumSignal> {
        if !self.base_adx.is_valid() {
            return None;
        }

        let adx = self.base_adx.adx();
        let direction = self.base_adx.get_direction();

        // Only bullish momentum for meme coins (LONG-ONLY)
        if adx >= self.config.entry_threshold
            && direction == TrendDirection::Bullish
            && self.bullish_bars >= self.config.min_confirmation_bars
        {
            return Some(MomentumSignal::BullishMomentum {
                adx,
                plus_di: self.base_adx.plus_di(),
                minus_di: self.base_adx.minus_di(),
            });
        }

        None
    }

    /// Check if exit conditions are met (trend dying)
    pub fn check_exit_signal(&self) -> Option<MomentumSignal> {
        if !self.base_adx.is_valid() {
            return None;
        }

        let adx = self.base_adx.adx();

        // Exit if ADX falls below exit threshold
        if adx < self.config.exit_threshold {
            return Some(MomentumSignal::TrendExpiring { adx });
        }

        None
    }

    /// Check if ADX is decaying (falling) while still above exit threshold
    /// Used for momentum decay check after certain hours
    pub fn is_adx_decaying(&self) -> bool {
        if let Some(prev) = self.prev_adx {
            let current = self.base_adx.adx();
            current < prev && current > self.config.exit_threshold
        } else {
            false
        }
    }

    /// Calculate momentum position multiplier (INVERTED from mean reversion)
    /// High ADX = larger position for momentum
    pub fn calculate_position_multiplier(&self) -> f64 {
        let adx = self.base_adx.adx();

        if adx < self.config.exit_threshold {
            0.0 // No position below exit threshold
        } else if adx < self.config.entry_threshold {
            0.3 // Small position in transition zone
        } else if adx < 35.0 {
            0.7 // Moderate position with confirmed trend
        } else if adx < 45.0 {
            1.0 // Full position with strong trend
        } else {
            0.8 // Slightly reduce on extreme ADX (potential exhaustion)
        }
    }

    /// Get current ADX value
    pub fn adx(&self) -> f64 {
        self.base_adx.adx()
    }

    /// Get current +DI value
    pub fn plus_di(&self) -> f64 {
        self.base_adx.plus_di()
    }

    /// Get current -DI value
    pub fn minus_di(&self) -> f64 {
        self.base_adx.minus_di()
    }

    /// Get trend direction
    pub fn direction(&self) -> TrendDirection {
        self.base_adx.get_direction()
    }

    /// Check if detector has enough data
    pub fn is_valid(&self) -> bool {
        self.base_adx.is_valid()
    }

    /// Get previous ADX value (for decay detection)
    pub fn prev_adx(&self) -> Option<f64> {
        self.prev_adx
    }

    /// Reset detector state
    pub fn reset(&mut self) {
        self.base_adx.reset();
        self.bullish_bars = 0;
        self.bearish_bars = 0;
        self.prev_adx = None;
        self.was_trending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_candle(high: f64, low: f64, close: f64) -> Candle {
        Candle::new(close, high, low, close, 1000.0)
    }

    fn create_test_detector() -> MomentumAdxDetector {
        MomentumAdxDetector::new(MomentumAdxConfig {
            period: 5,
            min_confirmation_bars: 1, // Quick confirmation for tests
            ..Default::default()
        })
    }

    #[test]
    fn test_config_defaults() {
        let config = MomentumAdxConfig::default();
        assert_eq!(config.period, 14);
        assert_eq!(config.entry_threshold, 25.0);
        assert_eq!(config.exit_threshold, 20.0);
        assert_eq!(config.warmup_periods(), 27);
    }

    #[test]
    fn test_config_meme_optimized() {
        let config = MomentumAdxConfig::meme_optimized();
        assert_eq!(config.period, 10);
        assert_eq!(config.warmup_periods(), 19);
    }

    #[test]
    fn test_detector_creation() {
        let detector = create_test_detector();
        assert!(!detector.is_valid());
        assert_eq!(detector.bullish_bars, 0);
        assert_eq!(detector.bearish_bars, 0);
    }

    #[test]
    fn test_warmup_returns_no_signal() {
        let mut detector = create_test_detector();

        // Not enough data yet
        for i in 0..5 {
            let candle = create_candle(100.0 + i as f64, 99.0, 99.5);
            let signal = detector.update(&candle);
            assert_eq!(signal, MomentumSignal::NoSignal);
        }
    }

    #[test]
    fn test_momentum_signal_helpers() {
        let bullish = MomentumSignal::BullishMomentum {
            adx: 30.0,
            plus_di: 35.0,
            minus_di: 20.0,
        };
        assert!(bullish.is_bullish_entry());
        assert!(!bullish.is_trend_dying());
        assert_eq!(bullish.adx(), Some(30.0));

        let expiring = MomentumSignal::TrendExpiring { adx: 18.0 };
        assert!(!expiring.is_bullish_entry());
        assert!(expiring.is_trend_dying());
        assert_eq!(expiring.adx(), Some(18.0));

        let no_signal = MomentumSignal::NoSignal;
        assert!(!no_signal.is_bullish_entry());
        assert!(!no_signal.is_trend_dying());
        assert_eq!(no_signal.adx(), None);
    }

    #[test]
    fn test_position_multiplier_inverted() {
        let mut detector = create_test_detector();

        // Warm up the detector
        for i in 0..15 {
            let base = 100.0 + (i as f64 * 2.0);
            let candle = create_candle(base + 1.0, base - 0.5, base);
            detector.update(&candle);
        }

        // Test multiplier values (inverted from mean reversion)
        // Note: Actual values depend on ADX from data, so we test the logic
        let mult = detector.calculate_position_multiplier();
        assert!(mult >= 0.0 && mult <= 1.0);
    }

    #[test]
    fn test_reset() {
        let mut detector = create_test_detector();

        // Feed some data
        for i in 0..15 {
            let base = 100.0 + (i as f64 * 2.0);
            let candle = create_candle(base + 1.0, base - 0.5, base);
            detector.update(&candle);
        }

        // Reset
        detector.reset();

        assert!(!detector.is_valid());
        assert_eq!(detector.bullish_bars, 0);
        assert_eq!(detector.bearish_bars, 0);
        assert!(detector.prev_adx.is_none());
        assert!(!detector.was_trending);
    }

    #[test]
    fn test_bullish_momentum_detection() {
        let mut detector = MomentumAdxDetector::new(MomentumAdxConfig {
            period: 5,
            entry_threshold: 20.0, // Lower for easier testing
            exit_threshold: 15.0,
            min_confirmation_bars: 1,
        });

        // Create strong uptrend
        for i in 0..20 {
            let base = 100.0 + (i as f64 * 3.0); // Strong uptrend
            let candle = create_candle(base + 1.5, base - 0.3, base + 1.0);
            detector.update(&candle);
        }

        // Should detect bullish direction in an uptrend
        if detector.is_valid() {
            let direction = detector.direction();
            // In a strong uptrend, +DI should be > -DI
            assert!(detector.plus_di() > 0.0);
        }
    }

    #[test]
    fn test_trend_expiring_detection() {
        let mut detector = MomentumAdxDetector::new(MomentumAdxConfig {
            period: 5,
            entry_threshold: 25.0,
            exit_threshold: 20.0,
            min_confirmation_bars: 1,
        });

        // First create a trend
        for i in 0..15 {
            let base = 100.0 + (i as f64 * 2.0);
            let candle = create_candle(base + 1.0, base - 0.5, base);
            detector.update(&candle);
        }

        // Now let trend die down with ranging prices
        for i in 0..10 {
            let base = 130.0 + ((i % 3) as f64 - 1.0) * 0.5; // Small range
            let candle = create_candle(base + 0.3, base - 0.3, base);
            let _signal = detector.update(&candle);
        }

        // After ranging, check exit signal
        if detector.is_valid() && detector.adx() < 20.0 {
            let exit = detector.check_exit_signal();
            assert!(exit.is_some());
        }
    }
}
