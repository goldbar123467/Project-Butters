//! Candle Builder - Builds OHLC candles from price ticks
//!
//! Accumulates price data over a configurable period and emits
//! complete candles for use with regime detection indicators.

use std::time::{Duration, Instant};
use super::Candle;

/// Builds OHLC candles from streaming price ticks
#[derive(Debug)]
pub struct CandleBuilder {
    /// Candle period duration
    period: Duration,
    /// Current candle start time
    candle_start: Option<Instant>,
    /// Current candle OHLC values
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    /// Accumulated volume (tick count for simplicity)
    tick_count: u64,
}

impl CandleBuilder {
    /// Create a new candle builder with specified period
    pub fn new(period: Duration) -> Self {
        Self {
            period,
            candle_start: None,
            open: 0.0,
            high: f64::MIN,
            low: f64::MAX,
            close: 0.0,
            tick_count: 0,
        }
    }

    /// Create with default 1-minute candles
    pub fn one_minute() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Create with 5-minute candles (recommended for ADX)
    pub fn five_minute() -> Self {
        Self::new(Duration::from_secs(300))
    }

    /// Update with a new price tick
    /// Returns Some(Candle) if a candle completed, None otherwise
    pub fn update(&mut self, price: f64) -> Option<Candle> {
        let now = Instant::now();

        // Initialize first candle
        if self.candle_start.is_none() {
            self.start_new_candle(price, now);
            return None;
        }

        let candle_start = self.candle_start.unwrap();

        // Check if current candle period has elapsed
        if now.duration_since(candle_start) >= self.period {
            // Close current candle and return it
            let completed = self.close_candle();

            // Start new candle with current price
            self.start_new_candle(price, now);

            return Some(completed);
        }

        // Update current candle
        self.high = self.high.max(price);
        self.low = self.low.min(price);
        self.close = price;
        self.tick_count += 1;

        None
    }

    /// Force close current candle and return it (useful for shutdown)
    pub fn force_close(&mut self) -> Option<Candle> {
        if self.candle_start.is_some() && self.tick_count > 0 {
            let candle = self.close_candle();
            self.candle_start = None;
            Some(candle)
        } else {
            None
        }
    }

    /// Start a new candle
    fn start_new_candle(&mut self, price: f64, start_time: Instant) {
        self.candle_start = Some(start_time);
        self.open = price;
        self.high = price;
        self.low = price;
        self.close = price;
        self.tick_count = 1;
    }

    /// Close current candle and return it
    fn close_candle(&self) -> Candle {
        Candle::new(
            self.open,
            self.high,
            self.low,
            self.close,
            self.tick_count as f64, // Use tick count as volume proxy
        )
    }

    /// Check if a candle is currently building
    pub fn is_building(&self) -> bool {
        self.candle_start.is_some()
    }

    /// Get current candle duration
    pub fn current_duration(&self) -> Option<Duration> {
        self.candle_start.map(|start| start.elapsed())
    }

    /// Get the candle period
    pub fn period(&self) -> Duration {
        self.period
    }

    /// Reset the builder
    pub fn reset(&mut self) {
        self.candle_start = None;
        self.open = 0.0;
        self.high = f64::MIN;
        self.low = f64::MAX;
        self.close = 0.0;
        self.tick_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_candle_builder_creation() {
        let builder = CandleBuilder::new(Duration::from_secs(60));
        assert_eq!(builder.period(), Duration::from_secs(60));
        assert!(!builder.is_building());
    }

    #[test]
    fn test_first_tick_starts_candle() {
        let mut builder = CandleBuilder::new(Duration::from_millis(100));

        let result = builder.update(100.0);
        assert!(result.is_none()); // First tick doesn't complete a candle
        assert!(builder.is_building());
    }

    #[test]
    fn test_candle_completes_after_period() {
        let mut builder = CandleBuilder::new(Duration::from_millis(50));

        // Start candle
        builder.update(100.0);

        // Wait for period to elapse
        thread::sleep(Duration::from_millis(60));

        // Next tick should complete the candle
        let candle = builder.update(105.0);
        assert!(candle.is_some());

        let c = candle.unwrap();
        assert_eq!(c.open, 100.0);
        assert_eq!(c.close, 100.0); // Close is from previous candle
    }

    #[test]
    fn test_high_low_tracking() {
        let mut builder = CandleBuilder::new(Duration::from_millis(100));

        builder.update(100.0); // Open
        builder.update(110.0); // New high
        builder.update(95.0);  // New low
        builder.update(105.0); // Close

        // Force close to get the candle
        let candle = builder.force_close().unwrap();

        assert_eq!(candle.open, 100.0);
        assert_eq!(candle.high, 110.0);
        assert_eq!(candle.low, 95.0);
        assert_eq!(candle.close, 105.0);
        assert_eq!(candle.volume, 4.0); // 4 ticks
    }

    #[test]
    fn test_force_close() {
        let mut builder = CandleBuilder::new(Duration::from_secs(60));

        builder.update(100.0);
        builder.update(105.0);

        let candle = builder.force_close();
        assert!(candle.is_some());

        // After force close, should not be building
        assert!(builder.candle_start.is_none());
    }

    #[test]
    fn test_reset() {
        let mut builder = CandleBuilder::new(Duration::from_secs(60));

        builder.update(100.0);
        assert!(builder.is_building());

        builder.reset();
        assert!(!builder.is_building());
    }

    #[test]
    fn test_preset_periods() {
        let one_min = CandleBuilder::one_minute();
        assert_eq!(one_min.period(), Duration::from_secs(60));

        let five_min = CandleBuilder::five_minute();
        assert_eq!(five_min.period(), Duration::from_secs(300));
    }
}
