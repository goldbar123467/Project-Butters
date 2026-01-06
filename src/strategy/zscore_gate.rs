//! Z-Score Gate
//!
//! Statistical filter that calculates z-scores from price data
//! to identify extreme deviations from the mean.
//!
//! Z-Score Formula: z = (current_price - rolling_mean) / rolling_std
//!
//! At z_threshold = 2.5:
//! - Only ~1.2% of data points in a normal distribution
//! - Extreme deviations revert with 65-75% probability

use crate::strategy::params::StrategyConfig;

/// Result of z-score calculation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZScoreResult {
    /// Current z-score value
    pub z_score: f64,
    /// Rolling mean used in calculation
    pub mean: f64,
    /// Rolling standard deviation
    pub std_dev: f64,
    /// Current price
    pub current_price: f64,
}

impl ZScoreResult {
    /// Check if z-score indicates oversold (below negative threshold)
    pub fn is_oversold(&self, threshold: f64) -> bool {
        self.z_score < -threshold
    }

    /// Check if z-score indicates overbought (above positive threshold)
    pub fn is_overbought(&self, threshold: f64) -> bool {
        self.z_score > threshold
    }

    /// Check if price is within normal range
    pub fn is_neutral(&self, threshold: f64) -> bool {
        self.z_score.abs() <= threshold
    }

    /// Distance from mean in terms of standard deviations
    pub fn deviation_magnitude(&self) -> f64 {
        self.z_score.abs()
    }
}

/// Z-Score calculation gate for mean reversion signals
#[derive(Debug, Clone)]
pub struct ZScoreGate {
    /// Configuration for the gate
    config: StrategyConfig,
    /// Price history buffer
    price_buffer: Vec<f64>,
}

impl ZScoreGate {
    /// Create a new z-score gate with the given configuration
    pub fn new(config: StrategyConfig) -> Self {
        let capacity = config.lookback_period;
        Self {
            config,
            price_buffer: Vec::with_capacity(capacity),
        }
    }

    /// Add a new price to the buffer and calculate z-score
    pub fn update(&mut self, price: f64) -> Option<ZScoreResult> {
        self.price_buffer.push(price);

        // Keep buffer at lookback_period size
        if self.price_buffer.len() > self.config.lookback_period {
            self.price_buffer.remove(0);
        }

        // Need full buffer for calculation
        if self.price_buffer.len() < self.config.lookback_period {
            return None;
        }

        self.calculate()
    }

    /// Calculate z-score from current buffer
    pub fn calculate(&self) -> Option<ZScoreResult> {
        if self.price_buffer.len() < self.config.lookback_period {
            return None;
        }

        let mean = self.rolling_mean();
        let std_dev = self.rolling_std(mean);

        // Avoid division by zero
        if std_dev < 1e-10 {
            return None;
        }

        let current_price = *self.price_buffer.last()?;
        let z_score = (current_price - mean) / std_dev;

        Some(ZScoreResult {
            z_score,
            mean,
            std_dev,
            current_price,
        })
    }

    /// Calculate rolling mean of price buffer
    fn rolling_mean(&self) -> f64 {
        let sum: f64 = self.price_buffer.iter().sum();
        sum / self.price_buffer.len() as f64
    }

    /// Calculate rolling standard deviation
    fn rolling_std(&self, mean: f64) -> f64 {
        let variance: f64 = self.price_buffer
            .iter()
            .map(|&price| {
                let diff = price - mean;
                diff * diff
            })
            .sum::<f64>() / self.price_buffer.len() as f64;

        variance.sqrt()
    }

    /// Get the current z-threshold from config
    pub fn threshold(&self) -> f64 {
        self.config.z_threshold
    }

    /// Check if current state indicates a buy signal
    pub fn should_buy(&self) -> bool {
        self.calculate()
            .map(|r| r.is_oversold(self.config.z_threshold))
            .unwrap_or(false)
    }

    /// Check if current state indicates a sell signal
    pub fn should_sell(&self) -> bool {
        self.calculate()
            .map(|r| r.is_overbought(self.config.z_threshold))
            .unwrap_or(false)
    }

    /// Reset the price buffer
    pub fn reset(&mut self) {
        self.price_buffer.clear();
    }

    /// Get number of prices in buffer
    pub fn buffer_len(&self) -> usize {
        self.price_buffer.len()
    }

    /// Check if buffer is full
    pub fn is_ready(&self) -> bool {
        self.price_buffer.len() >= self.config.lookback_period
    }

    /// Get the current price buffer (for testing/debugging)
    pub fn prices(&self) -> &[f64] {
        &self.price_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_gate() -> ZScoreGate {
        let config = StrategyConfig {
            lookback_period: 10,
            z_threshold: 2.0,
            ..Default::default()
        };
        ZScoreGate::new(config)
    }

    #[test]
    fn test_zscore_gate_creation() {
        let gate = create_test_gate();
        assert_eq!(gate.buffer_len(), 0);
        assert!(!gate.is_ready());
    }

    #[test]
    fn test_zscore_buffer_filling() {
        let mut gate = create_test_gate();

        // Fill buffer partially
        for i in 1..=5 {
            let result = gate.update(100.0 + i as f64);
            assert!(result.is_none()); // Not enough data yet
        }
        assert_eq!(gate.buffer_len(), 5);
        assert!(!gate.is_ready());

        // Fill to capacity
        for i in 6..=10 {
            gate.update(100.0 + i as f64);
        }
        assert!(gate.is_ready());
        assert!(gate.calculate().is_some());
    }

    #[test]
    fn test_zscore_calculation() {
        let mut gate = create_test_gate();

        // Add 10 prices with known mean (105) and std
        let prices = [100.0, 101.0, 102.0, 103.0, 104.0,
                      106.0, 107.0, 108.0, 109.0, 110.0];

        for &price in &prices {
            gate.update(price);
        }

        let result = gate.calculate().unwrap();
        assert!((result.mean - 105.0).abs() < 0.01);
        assert!(result.std_dev > 0.0);
        // Last price is 110, mean is 105, so z-score should be positive
        assert!(result.z_score > 0.0);
    }

    #[test]
    fn test_oversold_detection() {
        let mut gate = create_test_gate();

        // Create prices where the last one is significantly below mean
        let prices = [100.0; 9];
        for &price in &prices {
            gate.update(price);
        }
        // Add a much lower price
        gate.update(90.0);

        let result = gate.calculate().unwrap();
        // With mostly 100s and one 90, the mean is ~99, and 90 is below
        assert!(result.z_score < 0.0);
    }

    #[test]
    fn test_overbought_detection() {
        let mut gate = create_test_gate();

        // Create prices where the last one is significantly above mean
        let prices = [100.0; 9];
        for &price in &prices {
            gate.update(price);
        }
        // Add a much higher price
        gate.update(110.0);

        let result = gate.calculate().unwrap();
        assert!(result.z_score > 0.0);
    }

    #[test]
    fn test_buffer_rolling() {
        let mut gate = create_test_gate();

        // Fill buffer
        for i in 1..=10 {
            gate.update(i as f64);
        }
        assert_eq!(gate.buffer_len(), 10);

        // Add more - should roll
        gate.update(11.0);
        assert_eq!(gate.buffer_len(), 10);

        // First element should now be 2.0, not 1.0
        assert_eq!(gate.prices()[0], 2.0);
    }

    #[test]
    fn test_reset() {
        let mut gate = create_test_gate();

        for i in 1..=10 {
            gate.update(i as f64);
        }
        assert!(gate.is_ready());

        gate.reset();
        assert_eq!(gate.buffer_len(), 0);
        assert!(!gate.is_ready());
    }

    #[test]
    fn test_zscore_result_methods() {
        let result = ZScoreResult {
            z_score: -2.5,
            mean: 100.0,
            std_dev: 2.0,
            current_price: 95.0,
        };

        assert!(result.is_oversold(2.0));
        assert!(!result.is_overbought(2.0));
        assert!(!result.is_neutral(2.0));
        assert_eq!(result.deviation_magnitude(), 2.5);
    }
}
