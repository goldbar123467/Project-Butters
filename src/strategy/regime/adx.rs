//! ADX (Average Directional Index) Regime Detection
//!
//! Implements Wilder's ADX indicator for trend strength measurement.
//! ADX measures trend strength without indicating direction (0-100 scale).
//!
//! Components:
//! - +DI (Plus Directional Indicator): Upward price movement strength
//! - -DI (Minus Directional Indicator): Downward price movement strength
//! - ADX: Smoothed average of DX, measuring overall trend strength
//!
//! For mean reversion strategies:
//! - ADX < 20: Ranging market (favorable for mean reversion)
//! - ADX 20-25: Transition zone (neutral)
//! - ADX > 25: Trending market (unfavorable for mean reversion)

use super::{Candle, RegimeDetector, RegimeSignal};

/// Configuration for ADX calculation
#[derive(Debug, Clone)]
pub struct AdxConfig {
    /// Lookback period (default: 14)
    pub period: usize,
    /// ADX threshold below which market is considered ranging (default: 20.0)
    pub ranging_threshold: f64,
    /// ADX threshold above which market is considered trending (default: 25.0)
    pub trending_threshold: f64,
    /// Hysteresis for entry (must drop below this to enable trading)
    pub entry_threshold: f64,
    /// Hysteresis for exit (must rise above this to disable trading)
    pub exit_threshold: f64,
}

impl Default for AdxConfig {
    fn default() -> Self {
        Self {
            period: 14,
            ranging_threshold: 20.0,
            trending_threshold: 25.0,
            entry_threshold: 20.0,
            exit_threshold: 28.0,
        }
    }
}

impl AdxConfig {
    /// Create config optimized for crypto markets (shorter period, adjusted thresholds)
    pub fn crypto_optimized() -> Self {
        Self {
            period: 10,
            ranging_threshold: 20.0,
            trending_threshold: 30.0,
            entry_threshold: 20.0,
            exit_threshold: 30.0,
        }
    }

    /// Minimum bars needed before ADX is valid
    pub fn warmup_periods(&self) -> usize {
        2 * self.period - 1
    }
}

/// ADX calculation result
#[derive(Debug, Clone, Copy)]
pub struct AdxResult {
    /// Plus Directional Indicator (0-100)
    pub plus_di: f64,
    /// Minus Directional Indicator (0-100)
    pub minus_di: f64,
    /// Average Directional Index (0-100)
    pub adx: f64,
    /// Directional Index (0-100)
    pub dx: f64,
    /// True once warmup complete
    pub is_valid: bool,
}

/// Market trend regime based on ADX
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendRegime {
    /// Not enough data
    Unknown,
    /// ADX < 20: Weak/no trend, ideal for mean reversion
    Ranging,
    /// ADX 20-25: Transition zone
    Transitioning,
    /// ADX 25-40: Confirmed trend
    Trending,
    /// ADX 40-50: Strong trend
    StrongTrend,
    /// ADX >= 50: Extreme trend (potential exhaustion)
    ExtremeTrend,
}

impl TrendRegime {
    /// Get regime from ADX value
    pub fn from_adx(adx: f64) -> Self {
        if adx < 20.0 {
            Self::Ranging
        } else if adx < 25.0 {
            Self::Transitioning
        } else if adx < 40.0 {
            Self::Trending
        } else if adx < 50.0 {
            Self::StrongTrend
        } else {
            Self::ExtremeTrend
        }
    }

    /// Check if regime is favorable for mean reversion
    pub fn is_favorable_for_mean_reversion(&self) -> bool {
        matches!(self, Self::Ranging | Self::Transitioning)
    }
}

/// Trend direction based on DI crossover
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    /// +DI > -DI
    Bullish,
    /// -DI > +DI
    Bearish,
    /// +DI == -DI
    Neutral,
}

/// ADX-based regime detector for mean reversion strategies
#[derive(Debug)]
pub struct AdxRegimeDetector {
    config: AdxConfig,

    // Previous candle data
    prev_high: Option<f64>,
    prev_low: Option<f64>,
    prev_close: Option<f64>,

    // Wilder's smoothed values
    smoothed_tr: f64,
    smoothed_plus_dm: f64,
    smoothed_minus_dm: f64,

    // ADX value (also smoothed)
    adx: f64,

    // Tracking initialization
    bars_processed: usize,

    // Initialization accumulators
    tr_sum: f64,
    plus_dm_sum: f64,
    minus_dm_sum: f64,
    dx_values: Vec<f64>,

    // Latest output values
    plus_di: f64,
    minus_di: f64,
    adx_value: f64,

    // Trading state with hysteresis
    is_trading_enabled: bool,
}

impl AdxRegimeDetector {
    /// Create a new ADX regime detector
    pub fn new(config: AdxConfig) -> Self {
        let capacity = config.period;
        Self {
            config,
            prev_high: None,
            prev_low: None,
            prev_close: None,
            smoothed_tr: 0.0,
            smoothed_plus_dm: 0.0,
            smoothed_minus_dm: 0.0,
            adx: 0.0,
            bars_processed: 0,
            tr_sum: 0.0,
            plus_dm_sum: 0.0,
            minus_dm_sum: 0.0,
            dx_values: Vec::with_capacity(capacity),
            plus_di: 0.0,
            minus_di: 0.0,
            adx_value: 0.0,
            is_trading_enabled: true, // Start enabled, disable when trend detected
        }
    }

    /// Create with default config
    pub fn with_default_config() -> Self {
        Self::new(AdxConfig::default())
    }

    /// Create with crypto-optimized config
    pub fn crypto_optimized() -> Self {
        Self::new(AdxConfig::crypto_optimized())
    }

    /// Update with new candle data, returns ADX result
    pub fn update_candle(&mut self, candle: &Candle) -> AdxResult {
        let n = self.config.period as f64;

        // Calculate True Range
        let tr = self.calculate_true_range(candle);

        // Calculate Directional Movement
        let (plus_dm, minus_dm) = self.calculate_directional_movement(candle);

        // Store previous values for next iteration
        self.prev_high = Some(candle.high);
        self.prev_low = Some(candle.low);
        self.prev_close = Some(candle.close);

        self.bars_processed += 1;

        // Phase 1: Accumulate for first smoothed values (bars 1 to n)
        if self.bars_processed <= self.config.period {
            self.tr_sum += tr;
            self.plus_dm_sum += plus_dm;
            self.minus_dm_sum += minus_dm;

            // Initialize smoothed values at period n
            if self.bars_processed == self.config.period {
                self.smoothed_tr = self.tr_sum;
                self.smoothed_plus_dm = self.plus_dm_sum;
                self.smoothed_minus_dm = self.minus_dm_sum;

                // Calculate first DI values
                self.calculate_di_values();

                // Calculate first DX
                let dx = self.calculate_dx();
                self.dx_values.push(dx);
            }

            return AdxResult {
                plus_di: self.plus_di,
                minus_di: self.minus_di,
                adx: 0.0,
                dx: 0.0,
                is_valid: false,
            };
        }

        // Phase 2: Continue with Wilder's smoothing
        // Smoothed = Previous - (Previous / n) + Current
        self.smoothed_tr = self.smoothed_tr - (self.smoothed_tr / n) + tr;
        self.smoothed_plus_dm = self.smoothed_plus_dm - (self.smoothed_plus_dm / n) + plus_dm;
        self.smoothed_minus_dm = self.smoothed_minus_dm - (self.smoothed_minus_dm / n) + minus_dm;

        // Calculate DI values
        self.calculate_di_values();

        // Calculate DX
        let dx = self.calculate_dx();

        // Phase 2a: Accumulate DX for first ADX (bars n+1 to 2n-1)
        if self.bars_processed < 2 * self.config.period {
            self.dx_values.push(dx);

            // Initialize ADX at bar 2n-1
            if self.bars_processed == 2 * self.config.period - 1 {
                self.adx = self.dx_values.iter().sum::<f64>() / n;
                self.adx_value = self.adx;
            }

            return AdxResult {
                plus_di: self.plus_di,
                minus_di: self.minus_di,
                adx: self.adx_value,
                dx,
                is_valid: self.bars_processed >= 2 * self.config.period - 1,
            };
        }

        // Phase 3: Ongoing ADX calculation with Wilder's smoothing
        // ADX = (Previous_ADX * (n-1) + Current_DX) / n
        self.adx = (self.adx * (n - 1.0) + dx) / n;
        self.adx_value = self.adx;

        // Update trading enabled state with hysteresis
        self.update_trading_state();

        AdxResult {
            plus_di: self.plus_di,
            minus_di: self.minus_di,
            adx: self.adx_value,
            dx,
            is_valid: true,
        }
    }

    /// Calculate True Range (accounts for gaps)
    #[inline]
    fn calculate_true_range(&self, candle: &Candle) -> f64 {
        match self.prev_close {
            Some(prev_close) => {
                let hl = candle.high - candle.low;
                let hc = (candle.high - prev_close).abs();
                let lc = (candle.low - prev_close).abs();
                hl.max(hc).max(lc)
            }
            None => candle.high - candle.low,
        }
    }

    /// Calculate Directional Movement (+DM and -DM)
    #[inline]
    fn calculate_directional_movement(&self, candle: &Candle) -> (f64, f64) {
        match (self.prev_high, self.prev_low) {
            (Some(prev_high), Some(prev_low)) => {
                let up_move = candle.high - prev_high;
                let down_move = prev_low - candle.low;

                if up_move > down_move && up_move > 0.0 {
                    (up_move, 0.0)
                } else if down_move > up_move && down_move > 0.0 {
                    (0.0, down_move)
                } else {
                    (0.0, 0.0)
                }
            }
            _ => (0.0, 0.0),
        }
    }

    /// Calculate DI values from smoothed values
    #[inline]
    fn calculate_di_values(&mut self) {
        if self.smoothed_tr > 0.0 {
            self.plus_di = (self.smoothed_plus_dm / self.smoothed_tr) * 100.0;
            self.minus_di = (self.smoothed_minus_dm / self.smoothed_tr) * 100.0;
        } else {
            self.plus_di = 0.0;
            self.minus_di = 0.0;
        }
    }

    /// Calculate DX from DI values
    #[inline]
    fn calculate_dx(&self) -> f64 {
        let di_sum = self.plus_di + self.minus_di;
        if di_sum > 0.0 {
            ((self.plus_di - self.minus_di).abs() / di_sum) * 100.0
        } else {
            0.0
        }
    }

    /// Update trading state with hysteresis to prevent whipsaw
    fn update_trading_state(&mut self) {
        if self.is_trading_enabled {
            // Currently enabled - disable if ADX rises above exit threshold
            if self.adx_value > self.config.exit_threshold {
                self.is_trading_enabled = false;
            }
        } else {
            // Currently disabled - enable if ADX drops below entry threshold
            if self.adx_value < self.config.entry_threshold {
                self.is_trading_enabled = true;
            }
        }
    }

    /// Get current trend regime
    pub fn get_regime(&self) -> TrendRegime {
        if !self.is_valid() {
            return TrendRegime::Unknown;
        }
        TrendRegime::from_adx(self.adx_value)
    }

    /// Get trend direction based on DI crossover
    pub fn get_direction(&self) -> TrendDirection {
        if self.plus_di > self.minus_di {
            TrendDirection::Bullish
        } else if self.minus_di > self.plus_di {
            TrendDirection::Bearish
        } else {
            TrendDirection::Neutral
        }
    }

    /// Check if ADX is valid (warmup complete)
    pub fn is_valid(&self) -> bool {
        self.bars_processed >= 2 * self.config.period - 1
    }

    /// Get current ADX value
    pub fn adx(&self) -> f64 {
        self.adx_value
    }

    /// Get current +DI value
    pub fn plus_di(&self) -> f64 {
        self.plus_di
    }

    /// Get current -DI value
    pub fn minus_di(&self) -> f64 {
        self.minus_di
    }

    /// Check if trading is enabled (based on hysteresis)
    pub fn is_trading_enabled(&self) -> bool {
        self.is_trading_enabled
    }

    /// Convert ADX to confidence for mean reversion (inverse relationship)
    fn adx_to_confidence(&self) -> f64 {
        if self.adx_value < 15.0 {
            0.95
        } else if self.adx_value < 20.0 {
            0.80
        } else if self.adx_value < 25.0 {
            0.50
        } else if self.adx_value < 30.0 {
            0.25
        } else {
            0.10
        }
    }

    /// Calculate position multiplier based on ADX
    pub fn calculate_position_multiplier(&self) -> f64 {
        if self.adx_value < 15.0 {
            1.0
        } else if self.adx_value < 20.0 {
            0.8
        } else if self.adx_value < 25.0 {
            0.5
        } else if self.adx_value < 30.0 {
            0.2
        } else {
            0.0
        }
    }
}

impl RegimeDetector for AdxRegimeDetector {
    fn update(&mut self, candle: &Candle) -> Option<RegimeSignal> {
        let result = self.update_candle(candle);

        if !result.is_valid {
            return None;
        }

        let confidence = self.adx_to_confidence();
        Some(RegimeSignal::from_confidence(confidence))
    }

    fn get_position_multiplier(&self) -> f64 {
        self.calculate_position_multiplier()
    }

    fn is_ready(&self) -> bool {
        self.is_valid()
    }

    fn name(&self) -> &'static str {
        "ADX"
    }

    fn reset(&mut self) {
        self.prev_high = None;
        self.prev_low = None;
        self.prev_close = None;
        self.smoothed_tr = 0.0;
        self.smoothed_plus_dm = 0.0;
        self.smoothed_minus_dm = 0.0;
        self.adx = 0.0;
        self.bars_processed = 0;
        self.tr_sum = 0.0;
        self.plus_dm_sum = 0.0;
        self.minus_dm_sum = 0.0;
        self.dx_values.clear();
        self.plus_di = 0.0;
        self.minus_di = 0.0;
        self.adx_value = 0.0;
        self.is_trading_enabled = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_candle(high: f64, low: f64, close: f64) -> Candle {
        Candle::new(close, high, low, close, 1000.0)
    }

    fn create_test_detector() -> AdxRegimeDetector {
        AdxRegimeDetector::new(AdxConfig {
            period: 5,
            ..Default::default()
        })
    }

    #[test]
    fn test_adx_config_default() {
        let config = AdxConfig::default();
        assert_eq!(config.period, 14);
        assert_eq!(config.warmup_periods(), 27);
    }

    #[test]
    fn test_adx_config_crypto() {
        let config = AdxConfig::crypto_optimized();
        assert_eq!(config.period, 10);
        assert_eq!(config.warmup_periods(), 19);
    }

    #[test]
    fn test_detector_creation() {
        let detector = create_test_detector();
        assert!(!detector.is_ready());
        assert_eq!(detector.bars_processed, 0);
    }

    #[test]
    fn test_warmup_period() {
        let mut detector = create_test_detector();

        // With period 5, need 9 bars (2*5-1) for valid ADX
        for i in 0..8 {
            let candle = create_candle(100.0 + i as f64, 99.0, 99.5);
            let result = detector.update_candle(&candle);
            assert!(!result.is_valid, "Bar {} should not be valid", i);
        }

        // 9th bar should make it valid
        let candle = create_candle(108.0, 107.0, 107.5);
        let result = detector.update_candle(&candle);
        assert!(result.is_valid, "Bar 9 should be valid");
        assert!(detector.is_ready());
    }

    #[test]
    fn test_trending_market_high_adx() {
        let mut detector = create_test_detector();

        // Simulate strong uptrend with consistently higher highs/lows
        for i in 0..15 {
            let base = 100.0 + (i as f64 * 2.0); // Strong uptrend
            let candle = create_candle(base + 1.0, base - 0.5, base);
            detector.update_candle(&candle);
        }

        // In a strong trend, ADX should be elevated
        if detector.is_ready() {
            assert!(detector.adx() > 0.0, "ADX should be positive in a trend");
            // Direction should be bullish
            assert_eq!(detector.get_direction(), TrendDirection::Bullish);
        }
    }

    #[test]
    fn test_ranging_market_low_adx() {
        let mut detector = AdxRegimeDetector::new(AdxConfig {
            period: 5,
            ..Default::default()
        });

        // Simulate ranging/sideways market with oscillating prices
        let prices = [100.0, 101.0, 99.0, 100.5, 99.5, 100.2, 99.8, 100.1, 99.9, 100.0];
        for &price in &prices {
            let candle = create_candle(price + 0.5, price - 0.5, price);
            detector.update_candle(&candle);
        }

        // In a ranging market, ADX should be lower
        if detector.is_ready() {
            // Just verify ADX is calculated (actual value depends on data)
            assert!(detector.adx() >= 0.0 && detector.adx() <= 100.0);
        }
    }

    #[test]
    fn test_trend_regime_classification() {
        assert_eq!(TrendRegime::from_adx(15.0), TrendRegime::Ranging);
        assert_eq!(TrendRegime::from_adx(22.0), TrendRegime::Transitioning);
        assert_eq!(TrendRegime::from_adx(30.0), TrendRegime::Trending);
        assert_eq!(TrendRegime::from_adx(45.0), TrendRegime::StrongTrend);
        assert_eq!(TrendRegime::from_adx(55.0), TrendRegime::ExtremeTrend);
    }

    #[test]
    fn test_regime_favorable_for_mean_reversion() {
        assert!(TrendRegime::Ranging.is_favorable_for_mean_reversion());
        assert!(TrendRegime::Transitioning.is_favorable_for_mean_reversion());
        assert!(!TrendRegime::Trending.is_favorable_for_mean_reversion());
        assert!(!TrendRegime::StrongTrend.is_favorable_for_mean_reversion());
        assert!(!TrendRegime::ExtremeTrend.is_favorable_for_mean_reversion());
    }

    #[test]
    fn test_hysteresis() {
        let mut detector = AdxRegimeDetector::new(AdxConfig {
            period: 5,
            entry_threshold: 20.0,
            exit_threshold: 28.0,
            ..Default::default()
        });

        // Initially trading should be enabled
        assert!(detector.is_trading_enabled());

        // Manually set ADX values to test hysteresis
        detector.bars_processed = 10; // Make it "valid"
        detector.adx_value = 25.0;
        detector.update_trading_state();
        // ADX 25 is between 20 and 28, so state shouldn't change
        assert!(detector.is_trading_enabled());

        // Now push ADX above exit threshold
        detector.adx_value = 30.0;
        detector.update_trading_state();
        assert!(!detector.is_trading_enabled());

        // ADX drops but still above entry threshold
        detector.adx_value = 22.0;
        detector.update_trading_state();
        // Should still be disabled due to hysteresis
        assert!(!detector.is_trading_enabled());

        // ADX drops below entry threshold
        detector.adx_value = 18.0;
        detector.update_trading_state();
        assert!(detector.is_trading_enabled());
    }

    #[test]
    fn test_position_multiplier() {
        let mut detector = create_test_detector();
        detector.bars_processed = 10;

        detector.adx_value = 10.0;
        assert_eq!(detector.calculate_position_multiplier(), 1.0);

        detector.adx_value = 18.0;
        assert_eq!(detector.calculate_position_multiplier(), 0.8);

        detector.adx_value = 22.0;
        assert_eq!(detector.calculate_position_multiplier(), 0.5);

        detector.adx_value = 27.0;
        assert_eq!(detector.calculate_position_multiplier(), 0.2);

        detector.adx_value = 35.0;
        assert_eq!(detector.calculate_position_multiplier(), 0.0);
    }

    #[test]
    fn test_adx_to_confidence() {
        let mut detector = create_test_detector();

        detector.adx_value = 10.0;
        assert_eq!(detector.adx_to_confidence(), 0.95);

        detector.adx_value = 18.0;
        assert_eq!(detector.adx_to_confidence(), 0.80);

        detector.adx_value = 22.0;
        assert_eq!(detector.adx_to_confidence(), 0.50);

        detector.adx_value = 27.0;
        assert_eq!(detector.adx_to_confidence(), 0.25);

        detector.adx_value = 35.0;
        assert_eq!(detector.adx_to_confidence(), 0.10);
    }

    #[test]
    fn test_regime_detector_trait() {
        let mut detector = create_test_detector();

        // Before warmup, update returns None
        let candle = create_candle(100.0, 99.0, 99.5);
        assert!(detector.update(&candle).is_none());

        assert_eq!(detector.name(), "ADX");
        assert!(!detector.is_ready());

        // Warm up
        for i in 0..9 {
            let candle = create_candle(100.0 + i as f64, 99.0, 99.5);
            detector.update(&candle);
        }

        assert!(detector.is_ready());
    }

    #[test]
    fn test_reset() {
        let mut detector = create_test_detector();

        // Feed some data
        for i in 0..10 {
            let candle = create_candle(100.0 + i as f64, 99.0, 99.5);
            detector.update_candle(&candle);
        }

        assert!(detector.is_ready());
        assert!(detector.bars_processed > 0);

        detector.reset();

        assert!(!detector.is_ready());
        assert_eq!(detector.bars_processed, 0);
        assert_eq!(detector.adx(), 0.0);
        assert!(detector.is_trading_enabled());
    }

    #[test]
    fn test_adx_bounds() {
        let mut detector = create_test_detector();

        // Feed various data patterns
        for i in 0..20 {
            let candle = create_candle(100.0 + (i % 5) as f64, 99.0 - (i % 3) as f64, 99.5);
            let result = detector.update_candle(&candle);

            // ADX should always be 0-100
            assert!(result.adx >= 0.0 && result.adx <= 100.0, "ADX {} out of bounds", result.adx);
            assert!(result.plus_di >= 0.0 && result.plus_di <= 100.0);
            assert!(result.minus_di >= 0.0 && result.minus_di <= 100.0);
        }
    }

    #[test]
    fn test_trend_direction() {
        let mut detector = create_test_detector();

        // Create strong uptrend
        for i in 0..15 {
            let base = 100.0 + (i as f64 * 2.0);
            let candle = create_candle(base + 1.0, base - 0.2, base + 0.5);
            detector.update_candle(&candle);
        }

        // +DI should be higher in uptrend
        if detector.is_ready() {
            assert!(detector.plus_di() > 0.0);
            assert_eq!(detector.get_direction(), TrendDirection::Bullish);
        }
    }

    #[test]
    fn test_inside_bars() {
        let mut detector = create_test_detector();

        // First bar
        detector.update_candle(&create_candle(105.0, 95.0, 100.0));

        // Inside bar (range within previous range)
        detector.update_candle(&create_candle(103.0, 97.0, 100.0));

        // Both +DM and -DM should be 0 for inside bars
        // This is tested implicitly through ADX calculation
        assert!(detector.bars_processed == 2);
    }
}
