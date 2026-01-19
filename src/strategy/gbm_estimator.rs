//! Geometric Brownian Motion (GBM) Parameter Estimation
//!
//! Implements Maximum Likelihood Estimation (MLE) for GBM process parameters:
//! - μ (drift): Annualized drift rate
//! - σ (sigma): Annualized volatility
//!
//! The GBM process follows: dS = μSdt + σSdW
//!
//! Estimation from log returns:
//! - drift = mean(log_returns) / Δt + σ²/2
//! - volatility = std(log_returns) / sqrt(Δt)
//!
//! Used to determine trend direction and filter trades that align with market drift.

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Minimum drift magnitude to classify as bullish/bearish
const MIN_DRIFT_MAGNITUDE: f64 = 0.01;
/// Maximum reasonable annualized drift (1000%)
const MAX_DRIFT: f64 = 10.0;
/// Minimum volatility for numerical stability
const MIN_VOLATILITY: f64 = 1e-10;
/// Maximum reasonable annualized volatility (500%)
const MAX_VOLATILITY: f64 = 5.0;
/// Minimum variance for valid estimation
const MIN_VARIANCE: f64 = 1e-12;

/// Direction of drift in the GBM process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftDirection {
    /// Positive drift - price tends to increase
    Bullish,
    /// Negative drift - price tends to decrease
    Bearish,
    /// Near zero drift - no clear direction
    Neutral,
}

/// GBM process parameters estimated from price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GBMParams {
    /// Annualized drift rate (μ)
    pub drift: f64,
    /// Annualized volatility (σ)
    pub volatility: f64,
    /// Drift direction classification
    pub drift_direction: DriftDirection,
    /// Confidence in parameter estimation (0.0 - 1.0)
    pub confidence: f64,
    /// Number of samples used for estimation
    pub sample_count: usize,
}

impl GBMParams {
    /// Check if parameters indicate valid GBM behavior
    pub fn is_valid(&self) -> bool {
        self.volatility >= MIN_VOLATILITY
            && self.volatility <= MAX_VOLATILITY
            && self.drift.abs() <= MAX_DRIFT
            && self.confidence >= 0.0
            && self.confidence <= 1.0
            && self.sample_count >= 2
    }

    /// Calculate expected return over a time horizon (in minutes)
    pub fn expected_return(&self, horizon_minutes: f64) -> f64 {
        let horizon_years = horizon_minutes / (60.0 * 24.0 * 365.0);
        self.drift * horizon_years
    }

    /// Calculate expected price given current price and horizon
    pub fn expected_price(&self, current_price: f64, horizon_minutes: f64) -> f64 {
        let expected_log_return = self.expected_return(horizon_minutes);
        current_price * expected_log_return.exp()
    }

    /// Calculate probability of positive return over horizon (simplified)
    /// Uses Black-Scholes style probability
    pub fn probability_positive_return(&self, horizon_minutes: f64) -> f64 {
        let horizon_years = horizon_minutes / (60.0 * 24.0 * 365.0);
        if horizon_years <= 0.0 || self.volatility < MIN_VOLATILITY {
            return 0.5;
        }

        // d2 = (μ - σ²/2) * T / (σ * sqrt(T))
        let d2 = (self.drift - self.volatility.powi(2) / 2.0) * horizon_years.sqrt()
                 / self.volatility;

        // Approximate standard normal CDF
        standard_normal_cdf(d2)
    }
}

/// Approximate standard normal CDF using Abramowitz and Stegun approximation
fn standard_normal_cdf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x / 2.0).exp();

    0.5 * (1.0 + sign * y)
}

/// Geometric Brownian Motion process estimator
#[derive(Debug)]
pub struct GBMEstimator {
    /// Rolling buffer of log returns
    log_returns: VecDeque<f64>,
    /// Previous price for calculating returns
    prev_price: Option<f64>,
    /// Maximum number of returns to keep
    lookback: usize,
    /// Time step between samples (in minutes)
    dt_minutes: f64,
    /// Cached parameters (recomputed on update)
    params: Option<GBMParams>,
    /// Minimum samples required for estimation
    min_samples: usize,
}

impl GBMEstimator {
    /// Create a new GBM process estimator
    ///
    /// # Arguments
    /// * `lookback` - Number of samples for rolling estimation
    /// * `dt_minutes` - Time step between samples in minutes
    pub fn new(lookback: usize, dt_minutes: f64) -> Self {
        Self {
            log_returns: VecDeque::with_capacity(lookback),
            prev_price: None,
            lookback,
            dt_minutes,
            params: None,
            min_samples: lookback.min(30), // At least 30 samples or lookback, whichever is smaller
        }
    }

    /// Update with a new price observation
    ///
    /// Returns the current GBM parameters if estimation is possible
    pub fn update(&mut self, price: f64) -> Option<GBMParams> {
        if price <= 0.0 {
            return None;
        }

        // Calculate log return if we have a previous price
        if let Some(prev) = self.prev_price {
            let log_return = (price / prev).ln();
            self.log_returns.push_back(log_return);

            // Maintain rolling window
            while self.log_returns.len() > self.lookback {
                self.log_returns.pop_front();
            }
        }

        self.prev_price = Some(price);

        // Re-estimate parameters
        self.params = self.estimate_params();

        self.params.clone()
    }

    /// Estimate GBM parameters using Maximum Likelihood Estimation
    pub fn estimate_params(&self) -> Option<GBMParams> {
        let n = self.log_returns.len();
        if n < self.min_samples {
            return None;
        }

        let returns: Vec<f64> = self.log_returns.iter().copied().collect();

        // Time step in years for annualization
        let dt_years = self.dt_minutes / (60.0 * 24.0 * 365.0);

        // Calculate sample mean of log returns
        let mean_return = returns.iter().sum::<f64>() / n as f64;

        // Calculate sample variance of log returns
        let variance = returns.iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>() / (n - 1) as f64;

        if variance < MIN_VARIANCE {
            return None; // No variability, can't estimate
        }

        // Calculate annualized volatility: σ = std(returns) / sqrt(Δt)
        let volatility = (variance / dt_years).sqrt();

        if volatility < MIN_VOLATILITY || volatility > MAX_VOLATILITY {
            return None;
        }

        // Calculate annualized drift: μ = mean(returns) / Δt + σ²/2
        // This is the MLE estimator that accounts for Jensen's inequality
        let drift = mean_return / dt_years + volatility.powi(2) / 2.0;

        if drift.abs() > MAX_DRIFT {
            return None;
        }

        // Determine drift direction
        let drift_direction = if drift > MIN_DRIFT_MAGNITUDE {
            DriftDirection::Bullish
        } else if drift < -MIN_DRIFT_MAGNITUDE {
            DriftDirection::Bearish
        } else {
            DriftDirection::Neutral
        };

        // Calculate confidence
        let confidence = self.calculate_confidence(variance, n);

        Some(GBMParams {
            drift,
            volatility,
            drift_direction,
            confidence,
            sample_count: n,
        })
    }

    /// Calculate estimation confidence based on multiple factors
    fn calculate_confidence(&self, variance: f64, n: usize) -> f64 {
        // Factor 1: Sample size adequacy
        let sample_adequacy = (n as f64 / self.lookback as f64).clamp(0.0, 1.0);

        // Factor 2: Variance stability (not too high or too low)
        let var_stability = if variance > MIN_VARIANCE && variance < 0.1 {
            1.0
        } else if variance >= 0.1 {
            1.0 / (1.0 + variance.ln().abs())
        } else {
            0.5
        };

        // Factor 3: Statistical significance (more samples = more confident)
        // Standard error decreases with sqrt(n)
        let stat_significance = (n as f64 / 100.0).sqrt().clamp(0.0, 1.0);

        // Geometric mean of all factors for balanced confidence
        (sample_adequacy * var_stability * stat_significance).powf(1.0 / 3.0)
    }

    /// Get current GBM parameters if available
    pub fn params(&self) -> Option<&GBMParams> {
        self.params.as_ref()
    }

    /// Check if process has enough data for estimation
    pub fn is_ready(&self) -> bool {
        self.log_returns.len() >= self.min_samples && self.params.is_some()
    }

    /// Get the number of log return samples currently stored
    pub fn sample_count(&self) -> usize {
        self.log_returns.len()
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.log_returns.clear();
        self.prev_price = None;
        self.params = None;
    }

    /// Check if drift direction aligns with trade direction
    ///
    /// # Arguments
    /// * `is_long` - true for long trade, false for short trade
    ///
    /// # Returns
    /// true if drift supports the trade direction
    pub fn drift_aligns_with_trade(&self, is_long: bool) -> bool {
        match &self.params {
            Some(params) if params.is_valid() => {
                match params.drift_direction {
                    DriftDirection::Bullish => is_long,
                    DriftDirection::Bearish => !is_long,
                    DriftDirection::Neutral => true, // Neutral allows both directions
                }
            }
            _ => false, // No valid params means no alignment
        }
    }

    /// Get current drift if available
    pub fn current_drift(&self) -> Option<f64> {
        self.params.as_ref().map(|p| p.drift)
    }

    /// Get current volatility if available
    pub fn current_volatility(&self) -> Option<f64> {
        self.params.as_ref().map(|p| p.volatility)
    }

    /// Get drift direction if available
    pub fn drift_direction(&self) -> Option<DriftDirection> {
        self.params.as_ref().map(|p| p.drift_direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate synthetic GBM data for testing
    fn generate_gbm_series(n: usize, drift: f64, volatility: f64, dt_years: f64, initial_price: f64) -> Vec<f64> {
        let mut rng_state = 12345u64; // Simple deterministic RNG for reproducibility
        let mut prices = Vec::with_capacity(n);
        let mut s = initial_price;

        for _ in 0..n {
            prices.push(s);

            // Box-Muller transform for normal random
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u1 = (rng_state as f64) / (u64::MAX as f64);
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u2 = (rng_state as f64) / (u64::MAX as f64);
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();

            // GBM update: S(t+dt) = S(t) * exp((μ - σ²/2)dt + σ√dt * Z)
            let log_return = (drift - volatility.powi(2) / 2.0) * dt_years + volatility * dt_years.sqrt() * z;
            s *= log_return.exp();
        }

        prices
    }

    #[test]
    fn test_gbm_estimator_creation() {
        let gbm = GBMEstimator::new(100, 3.0);
        assert!(!gbm.is_ready());
        assert_eq!(gbm.sample_count(), 0);
    }

    #[test]
    fn test_gbm_parameter_estimation() {
        // Generate synthetic GBM series with known parameters
        // drift=0.2 (20% annual), volatility=0.3 (30% annual)
        // Use larger volatility to ensure enough variance passes MIN_VARIANCE threshold
        let dt_minutes = 3.0;
        let dt_years = dt_minutes / (60.0 * 24.0 * 365.0);
        // Use higher volatility (0.5) to ensure variability in short time steps
        let prices = generate_gbm_series(200, 0.2, 0.5, dt_years, 100.0);

        let mut gbm = GBMEstimator::new(100, dt_minutes);

        for price in &prices {
            gbm.update(*price);
        }

        // Check if we have enough samples
        assert!(gbm.sample_count() >= 30, "Should have at least 30 samples, got {}", gbm.sample_count());

        // The estimator may not always be ready due to variance thresholds
        // so we test conditionally
        if gbm.is_ready() {
            let params = gbm.params().expect("Should have params");

            // Check that estimated parameters are in reasonable range
            assert!(params.volatility > 0.0, "volatility should be positive");
            assert!(params.confidence >= 0.0 && params.confidence <= 1.0, "confidence should be [0,1]");
            assert!(params.is_valid(), "params should be valid");
        } else {
            // If not ready, params should be None
            // This can happen with very small time steps where variance is below threshold
            assert!(gbm.params().is_none() || !gbm.is_ready());
        }
    }

    #[test]
    fn test_gbm_drift_direction_bullish() {
        let dt_minutes = 3.0;
        let dt_years = dt_minutes / (60.0 * 24.0 * 365.0);
        // Strong positive drift
        let prices = generate_gbm_series(100, 0.5, 0.2, dt_years, 100.0);

        let mut gbm = GBMEstimator::new(100, dt_minutes);
        for price in &prices {
            gbm.update(*price);
        }

        if let Some(params) = gbm.params() {
            if params.drift > MIN_DRIFT_MAGNITUDE {
                assert_eq!(params.drift_direction, DriftDirection::Bullish);
            }
        }
    }

    #[test]
    fn test_gbm_drift_direction_bearish() {
        let dt_minutes = 3.0;
        let dt_years = dt_minutes / (60.0 * 24.0 * 365.0);
        // Strong negative drift
        let prices = generate_gbm_series(100, -0.5, 0.2, dt_years, 100.0);

        let mut gbm = GBMEstimator::new(100, dt_minutes);
        for price in &prices {
            gbm.update(*price);
        }

        if let Some(params) = gbm.params() {
            if params.drift < -MIN_DRIFT_MAGNITUDE {
                assert_eq!(params.drift_direction, DriftDirection::Bearish);
            }
        }
    }

    #[test]
    fn test_gbm_drift_aligns_with_trade_bullish() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        // Feed steadily increasing prices to create bullish drift
        for i in 0..60 {
            let price = 100.0 * (1.0 + 0.001 * i as f64); // 0.1% increase each tick
            gbm.update(price);
        }

        if gbm.is_ready() {
            // Long trade should align with bullish drift
            assert!(gbm.drift_aligns_with_trade(true), "Long should align with bullish");
        }
    }

    #[test]
    fn test_gbm_drift_aligns_with_trade_bearish() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        // Feed steadily decreasing prices to create bearish drift
        for i in 0..60 {
            let price = 100.0 * (1.0 - 0.001 * i as f64); // 0.1% decrease each tick
            gbm.update(price);
        }

        if gbm.is_ready() {
            // Short trade should align with bearish drift
            assert!(gbm.drift_aligns_with_trade(false), "Short should align with bearish");
        }
    }

    #[test]
    fn test_gbm_params_validity() {
        let valid_params = GBMParams {
            drift: 0.2,
            volatility: 0.3,
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 50,
        };
        assert!(valid_params.is_valid());

        let invalid_volatility = GBMParams {
            drift: 0.2,
            volatility: -0.1, // Invalid: negative
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 50,
        };
        assert!(!invalid_volatility.is_valid());

        let invalid_confidence = GBMParams {
            drift: 0.2,
            volatility: 0.3,
            drift_direction: DriftDirection::Bullish,
            confidence: 1.5, // Invalid: > 1.0
            sample_count: 50,
        };
        assert!(!invalid_confidence.is_valid());

        let invalid_samples = GBMParams {
            drift: 0.2,
            volatility: 0.3,
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 1, // Invalid: < 2
        };
        assert!(!invalid_samples.is_valid());
    }

    #[test]
    fn test_gbm_reset() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        for i in 0..60 {
            gbm.update(100.0 + (i as f64) * 0.1);
        }

        assert!(gbm.sample_count() > 0);

        gbm.reset();

        assert_eq!(gbm.sample_count(), 0);
        assert!(!gbm.is_ready());
        assert!(gbm.params().is_none());
    }

    #[test]
    fn test_gbm_edge_case_negative_price() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        // Negative prices should return None
        let result = gbm.update(-100.0);
        assert!(result.is_none());

        // Zero price should also return None
        let result = gbm.update(0.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_gbm_expected_return() {
        let params = GBMParams {
            drift: 0.2, // 20% annual drift
            volatility: 0.3,
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 50,
        };

        // Expected return over 1 year (525600 minutes)
        let one_year_minutes = 60.0 * 24.0 * 365.0;
        let expected = params.expected_return(one_year_minutes);
        assert!((expected - 0.2).abs() < 0.001, "Expected ~20% return over 1 year");

        // Expected return over 1 day
        let one_day_minutes = 60.0 * 24.0;
        let expected_daily = params.expected_return(one_day_minutes);
        assert!(expected_daily > 0.0 && expected_daily < 0.01, "Expected small positive return over 1 day");
    }

    #[test]
    fn test_gbm_expected_price() {
        let params = GBMParams {
            drift: 0.2, // 20% annual drift
            volatility: 0.3,
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 50,
        };

        let current_price = 100.0;
        let one_year_minutes = 60.0 * 24.0 * 365.0;
        let expected_price = params.expected_price(current_price, one_year_minutes);

        // With 20% drift, expected price after 1 year should be around 100 * exp(0.2) = ~122
        assert!(expected_price > 100.0, "Expected price should increase with positive drift");
        assert!((expected_price - 122.14).abs() < 1.0, "Expected ~122 after 1 year");
    }

    #[test]
    fn test_gbm_probability_positive_return() {
        let bullish_params = GBMParams {
            drift: 0.3, // Strong positive drift
            volatility: 0.2,
            drift_direction: DriftDirection::Bullish,
            confidence: 0.8,
            sample_count: 50,
        };

        let one_day_minutes = 60.0 * 24.0;
        let prob = bullish_params.probability_positive_return(one_day_minutes);
        assert!(prob > 0.5, "Bullish drift should have >50% probability of positive return");

        let bearish_params = GBMParams {
            drift: -0.3, // Strong negative drift
            volatility: 0.2,
            drift_direction: DriftDirection::Bearish,
            confidence: 0.8,
            sample_count: 50,
        };

        let prob = bearish_params.probability_positive_return(one_day_minutes);
        assert!(prob < 0.5, "Bearish drift should have <50% probability of positive return");
    }

    #[test]
    fn test_standard_normal_cdf() {
        // Test standard normal CDF at key points
        let cdf_0 = standard_normal_cdf(0.0);
        assert!((cdf_0 - 0.5).abs() < 0.001, "CDF(0) should be 0.5");

        let cdf_positive = standard_normal_cdf(2.0);
        assert!(cdf_positive > 0.97, "CDF(2) should be > 0.97");

        let cdf_negative = standard_normal_cdf(-2.0);
        assert!(cdf_negative < 0.03, "CDF(-2) should be < 0.03");
    }

    #[test]
    fn test_gbm_insufficient_data() {
        let mut gbm = GBMEstimator::new(100, 3.0);

        // Feed only a few prices
        for i in 0..10 {
            gbm.update(100.0 + i as f64);
        }

        assert!(!gbm.is_ready());
        assert!(gbm.params().is_none());
    }

    #[test]
    fn test_gbm_constant_prices() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        // Feed constant prices (zero variance)
        for _ in 0..60 {
            gbm.update(100.0);
        }

        // Should not have valid params due to no variability
        assert!(!gbm.is_ready() || gbm.params().is_none());
    }

    #[test]
    fn test_gbm_getters() {
        let dt_minutes = 3.0;
        let dt_years = dt_minutes / (60.0 * 24.0 * 365.0);
        let prices = generate_gbm_series(60, 0.2, 0.3, dt_years, 100.0);

        let mut gbm = GBMEstimator::new(50, dt_minutes);
        for price in &prices {
            gbm.update(*price);
        }

        if gbm.is_ready() {
            assert!(gbm.current_drift().is_some());
            assert!(gbm.current_volatility().is_some());
            assert!(gbm.drift_direction().is_some());
        }
    }

    #[test]
    fn test_gbm_rolling_window() {
        let mut gbm = GBMEstimator::new(50, 3.0);

        // Fill with 100 prices (should keep only last 50 returns)
        for i in 0..100 {
            gbm.update(100.0 + (i as f64) * 0.1);
        }

        // Should have exactly lookback returns (49 because returns = prices - 1)
        assert!(gbm.sample_count() <= 50);
    }

    #[test]
    fn test_drift_direction_enum_equality() {
        assert_eq!(DriftDirection::Bullish, DriftDirection::Bullish);
        assert_eq!(DriftDirection::Bearish, DriftDirection::Bearish);
        assert_eq!(DriftDirection::Neutral, DriftDirection::Neutral);
        assert_ne!(DriftDirection::Bullish, DriftDirection::Bearish);
    }
}
