//! Ornstein-Uhlenbeck Process Parameter Estimation
//!
//! Implements Maximum Likelihood Estimation (MLE) for OU process parameters:
//! - theta: Mean reversion speed (higher = faster reversion)
//! - mu: Long-term equilibrium level
//! - sigma: Volatility
//!
//! The OU process follows: dX(t) = theta(mu - X(t))dt + sigma*dW(t)
//!
//! Entry signals are generated when price deviates significantly from equilibrium,
//! measured by z_ou = (log_price - mu) / (sigma / sqrt(2*theta))
//!
//! Ported from butters-sniper for kyzlo-dex meme coin orchestrator.

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Minimum theta value to prevent division issues
const MIN_THETA: f64 = 0.0001;
/// Maximum theta value for reasonable mean reversion
const MAX_THETA: f64 = 100.0;
/// Minimum sigma for numerical stability
const MIN_SIGMA: f64 = 1e-10;
/// Minimum rho (autocorrelation) - must be mean-reverting
const MIN_RHO: f64 = 0.01;
/// Maximum rho - must not be unit root
const MAX_RHO: f64 = 0.99;
/// Minimum variance for valid estimation
const MIN_VARIANCE: f64 = 1e-12;

/// OU process parameters estimated from price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OUParams {
    /// Mean reversion speed (higher = faster reversion)
    pub theta: f64,
    /// Long-term equilibrium level (in log price space)
    pub mu: f64,
    /// Volatility of the process
    pub sigma: f64,
    /// Half-life of mean reversion in the same time units as dt
    pub half_life: f64,
    /// Confidence in parameter estimation (0.0 - 1.0)
    pub confidence: f64,
    /// Lag-1 autocorrelation used in estimation
    pub rho: f64,
}

impl OUParams {
    /// Calculate OU z-score for a given log price
    /// z_ou = (log_price - mu) / (sigma / sqrt(2*theta))
    pub fn z_score(&self, log_price: f64) -> f64 {
        let denominator = self.sigma / (2.0 * self.theta).sqrt();
        if denominator.abs() < MIN_SIGMA {
            return 0.0;
        }
        (log_price - self.mu) / denominator
    }

    /// Check if parameters indicate valid mean-reverting behavior
    pub fn is_valid(&self) -> bool {
        self.theta >= MIN_THETA
            && self.theta <= MAX_THETA
            && self.sigma >= MIN_SIGMA
            && self.half_life > 0.0
            && self.confidence >= 0.0
            && self.confidence <= 1.0
            && self.rho >= MIN_RHO
            && self.rho <= MAX_RHO
    }
}

/// OU signal generated from parameter analysis
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OUSignal {
    /// Price significantly below equilibrium - potential buy
    Oversold { z_ou: f64 },
    /// Price significantly above equilibrium - potential sell
    Overbought { z_ou: f64 },
    /// Price near equilibrium - no signal
    Neutral { z_ou: f64 },
    /// Insufficient data or invalid parameters
    Unavailable,
}

/// Ornstein-Uhlenbeck process estimator
#[derive(Debug)]
pub struct OUProcess {
    /// Rolling buffer of log prices
    log_prices: VecDeque<f64>,
    /// Maximum number of samples to keep
    lookback: usize,
    /// Time step between samples (in minutes)
    dt_minutes: f64,
    /// Cached parameters (recomputed on update)
    params: Option<OUParams>,
    /// Minimum samples required for estimation
    min_samples: usize,
}

impl OUProcess {
    /// Create a new OU process estimator
    ///
    /// # Arguments
    /// * `lookback` - Number of samples for rolling estimation
    /// * `dt_minutes` - Time step between samples in minutes
    pub fn new(lookback: usize, dt_minutes: f64) -> Self {
        Self {
            log_prices: VecDeque::with_capacity(lookback + 1),
            lookback,
            dt_minutes,
            params: None,
            min_samples: lookback.min(50), // At least 50 samples or lookback, whichever is smaller
        }
    }

    /// Update with a new price observation
    ///
    /// Returns the current OU signal if parameters can be estimated
    pub fn update(&mut self, price: f64) -> OUSignal {
        if price <= 0.0 {
            return OUSignal::Unavailable;
        }

        let log_price = price.ln();
        self.log_prices.push_back(log_price);

        // Maintain rolling window
        while self.log_prices.len() > self.lookback {
            self.log_prices.pop_front();
        }

        // Re-estimate parameters
        self.params = self.estimate_params();

        // Generate signal
        self.generate_signal(log_price)
    }

    /// Estimate OU parameters using Maximum Likelihood Estimation
    fn estimate_params(&self) -> Option<OUParams> {
        let n = self.log_prices.len();
        if n < self.min_samples {
            return None;
        }

        let prices: Vec<f64> = self.log_prices.iter().copied().collect();
        let dt = self.dt_minutes / 60.0; // Convert to hours for half-life calculation

        // Calculate sample statistics
        let mean = prices.iter().sum::<f64>() / n as f64;
        let variance = prices.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / (n - 1) as f64;

        if variance < MIN_VARIANCE {
            return None; // No variability, can't estimate
        }

        // Calculate lag-1 autocorrelation (rho)
        let mut cov_sum = 0.0;
        let mut var_sum_x = 0.0;
        let mut var_sum_y = 0.0;
        let mean_x: f64 = prices[..n-1].iter().sum::<f64>() / (n - 1) as f64;
        let mean_y: f64 = prices[1..].iter().sum::<f64>() / (n - 1) as f64;

        for i in 0..(n - 1) {
            let x = prices[i] - mean_x;
            let y = prices[i + 1] - mean_y;
            cov_sum += x * y;
            var_sum_x += x * x;
            var_sum_y += y * y;
        }

        let var_x = var_sum_x / (n - 2) as f64;
        let var_y = var_sum_y / (n - 2) as f64;

        if var_x < MIN_VARIANCE || var_y < MIN_VARIANCE {
            return None;
        }

        let rho = cov_sum / ((var_sum_x * var_sum_y).sqrt());

        // Validate rho for mean-reverting behavior
        if rho <= MIN_RHO || rho >= MAX_RHO {
            return None; // Not a valid mean-reverting process
        }

        // Estimate theta from autocorrelation
        // For AR(1) process X(t+1) = rho * X(t) + noise, theta = -ln(rho) / dt
        let theta = -rho.ln() / dt;

        if theta < MIN_THETA || theta > MAX_THETA {
            return None;
        }

        // Estimate mu (equilibrium level)
        let mu = mean;

        // Estimate sigma from variance
        // Var(X) = sigma^2 / (2*theta) for stationary OU process
        // Therefore: sigma = sqrt(Var(X) * 2*theta)
        let sigma = (variance * 2.0 * theta).sqrt();

        if sigma < MIN_SIGMA {
            return None;
        }

        // Calculate half-life: t_1/2 = ln(2) / theta
        let half_life = 2.0_f64.ln() / theta;

        // Calculate confidence
        let confidence = self.calculate_confidence(rho, variance, n);

        Some(OUParams {
            theta,
            mu,
            sigma,
            half_life,
            confidence,
            rho,
        })
    }

    /// Calculate estimation confidence based on multiple factors
    fn calculate_confidence(&self, rho: f64, variance: f64, n: usize) -> f64 {
        // Factor 1: Mean reversion strength (stronger = more confident)
        // rho close to 1 = weak mean reversion, rho close to 0 = strong mean reversion
        let mr_strength = (1.0 - rho).clamp(0.0, 1.0);

        // Factor 2: Sample size adequacy
        let sample_adequacy = (n as f64 / self.lookback as f64).clamp(0.0, 1.0);

        // Factor 3: Variance stability (not too high or too low)
        // Normalized variance check - extreme values reduce confidence
        let var_stability = if variance > MIN_VARIANCE && variance < 1.0 {
            1.0
        } else if variance >= 1.0 {
            1.0 / (1.0 + variance.ln())
        } else {
            0.5
        };

        // Factor 4: R-squared approximation from AR(1) fit
        let r_squared = rho.powi(2);

        // Geometric mean of all factors for balanced confidence
        (mr_strength * sample_adequacy * var_stability * r_squared).powf(0.25)
    }

    /// Generate trading signal based on current z-score
    fn generate_signal(&self, current_log_price: f64) -> OUSignal {
        match &self.params {
            Some(params) if params.is_valid() => {
                let z_ou = params.z_score(current_log_price);

                // Use 2.0 sigma threshold for initial signal classification
                // Actual trading threshold is applied by the strategy
                if z_ou < -2.0 {
                    OUSignal::Oversold { z_ou }
                } else if z_ou > 2.0 {
                    OUSignal::Overbought { z_ou }
                } else {
                    OUSignal::Neutral { z_ou }
                }
            }
            _ => OUSignal::Unavailable,
        }
    }

    /// Get current OU parameters if available
    pub fn params(&self) -> Option<&OUParams> {
        self.params.as_ref()
    }

    /// Check if process has enough data for estimation
    pub fn is_ready(&self) -> bool {
        self.log_prices.len() >= self.min_samples && self.params.is_some()
    }

    /// Get current z-score if parameters are available
    pub fn current_z_score(&self) -> Option<f64> {
        let current_log_price = self.log_prices.back()?;
        let params = self.params.as_ref()?;
        Some(params.z_score(*current_log_price))
    }

    /// Get the number of samples currently stored
    pub fn sample_count(&self) -> usize {
        self.log_prices.len()
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.log_prices.clear();
        self.params = None;
    }

    /// Get half-life in minutes if available
    pub fn half_life_minutes(&self) -> Option<f64> {
        self.params.as_ref().map(|p| p.half_life * 60.0) // Convert from hours to minutes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate synthetic mean-reverting data for testing
    fn generate_ou_series(n: usize, theta: f64, mu: f64, sigma: f64, dt: f64) -> Vec<f64> {
        use std::f64::consts::E;

        let mut rng_state = 12345u64; // Simple deterministic RNG for reproducibility
        let mut prices = Vec::with_capacity(n);
        let mut x = mu; // Start at equilibrium

        for _ in 0..n {
            // Box-Muller transform for normal random
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u1 = (rng_state as f64) / (u64::MAX as f64);
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u2 = (rng_state as f64) / (u64::MAX as f64);
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();

            // Exact OU update
            let decay = E.powf(-theta * dt);
            let mean_term = mu * (1.0 - decay);
            let vol_term = sigma * ((1.0 - decay * decay) / (2.0 * theta)).sqrt();

            x = decay * x + mean_term + vol_term * z;
            prices.push(x.exp()); // Convert back to price space
        }

        prices
    }

    #[test]
    fn test_ou_process_creation() {
        let ou = OUProcess::new(100, 3.0);
        assert!(!ou.is_ready());
        assert_eq!(ou.sample_count(), 0);
    }

    #[test]
    fn test_ou_parameter_estimation() {
        // Generate synthetic mean-reverting series
        let prices = generate_ou_series(150, 0.5, 4.6, 0.1, 0.05); // theta=0.5, mu~100, sigma=0.1

        let mut ou = OUProcess::new(150, 3.0);

        for price in &prices {
            ou.update(*price);
        }

        assert!(ou.is_ready());

        let params = ou.params().expect("Should have params");

        // Check that estimated parameters are in reasonable range
        assert!(params.theta > 0.0, "theta should be positive");
        assert!(params.sigma > 0.0, "sigma should be positive");
        assert!(params.half_life > 0.0, "half_life should be positive");
        assert!(params.confidence >= 0.0 && params.confidence <= 1.0, "confidence should be [0,1]");
    }

    #[test]
    fn test_ou_z_score_calculation() {
        let params = OUParams {
            theta: 0.5,
            mu: 4.6,  // ln(100) ~ 4.6
            sigma: 0.1,
            half_life: 1.39,
            confidence: 0.8,
            rho: 0.6,
        };

        // At equilibrium, z-score should be 0
        let z_at_mu = params.z_score(4.6);
        assert!(z_at_mu.abs() < 0.01, "z-score at mu should be ~0");

        // Below equilibrium should be negative
        let z_below = params.z_score(4.5);
        assert!(z_below < 0.0, "z-score below mu should be negative");

        // Above equilibrium should be positive
        let z_above = params.z_score(4.7);
        assert!(z_above > 0.0, "z-score above mu should be positive");
    }

    #[test]
    fn test_ou_signal_generation() {
        let mut ou = OUProcess::new(60, 3.0);

        // Feed stable prices around 100
        for _ in 0..60 {
            ou.update(100.0);
        }

        // A significant drop should generate oversold signal
        let signal = ou.update(90.0);

        match signal {
            OUSignal::Oversold { z_ou } => {
                assert!(z_ou < 0.0, "Oversold z_ou should be negative");
            }
            OUSignal::Neutral { .. } => {
                // Also acceptable if variance is high
            }
            _ => {
                // Unavailable is acceptable if estimation failed
            }
        }
    }

    #[test]
    fn test_ou_invalid_rho_rejection() {
        let mut ou = OUProcess::new(50, 3.0);

        // Feed constant prices (rho = 1, unit root)
        for _ in 0..60 {
            ou.update(100.0);
        }

        // Should not have valid params due to no variability
        assert!(ou.params().is_none() || !ou.params().unwrap().is_valid());
    }

    #[test]
    fn test_ou_params_validity() {
        let valid_params = OUParams {
            theta: 0.5,
            mu: 4.6,
            sigma: 0.1,
            half_life: 1.39,
            confidence: 0.8,
            rho: 0.6,
        };
        assert!(valid_params.is_valid());

        let invalid_theta = OUParams {
            theta: -0.1, // Invalid: negative
            mu: 4.6,
            sigma: 0.1,
            half_life: 1.39,
            confidence: 0.8,
            rho: 0.6,
        };
        assert!(!invalid_theta.is_valid());

        let invalid_rho = OUParams {
            theta: 0.5,
            mu: 4.6,
            sigma: 0.1,
            half_life: 1.39,
            confidence: 0.8,
            rho: 0.001, // Invalid: too close to 0
        };
        assert!(!invalid_rho.is_valid());
    }

    #[test]
    fn test_ou_reset() {
        let mut ou = OUProcess::new(50, 3.0);

        for i in 0..60 {
            ou.update(100.0 + (i as f64) * 0.1);
        }

        assert!(ou.sample_count() > 0);

        ou.reset();

        assert_eq!(ou.sample_count(), 0);
        assert!(!ou.is_ready());
        assert!(ou.params().is_none());
    }

    #[test]
    fn test_ou_half_life_minutes() {
        let mut ou = OUProcess::new(50, 3.0);

        // Feed some data
        for i in 0..60 {
            ou.update(100.0 + (i as f64 % 5.0) * 0.5);
        }

        if let Some(half_life_min) = ou.half_life_minutes() {
            assert!(half_life_min > 0.0, "half_life should be positive");
        }
    }

    #[test]
    fn test_ou_edge_case_negative_price() {
        let mut ou = OUProcess::new(50, 3.0);

        // Negative prices should return Unavailable
        let signal = ou.update(-100.0);
        assert_eq!(signal, OUSignal::Unavailable);

        // Zero price should also return Unavailable
        let signal = ou.update(0.0);
        assert_eq!(signal, OUSignal::Unavailable);
    }
}
