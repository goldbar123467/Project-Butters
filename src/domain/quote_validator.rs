//! Quote Validator
//!
//! Cross-validates Jupiter quotes against reference prices and enforces
//! price impact limits to prevent unfavorable trades.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default maximum price impact percentage
pub const DEFAULT_MAX_PRICE_IMPACT_PCT: f64 = 2.0;

/// Default maximum deviation from oracle/reference price
pub const DEFAULT_MAX_ORACLE_DEVIATION_PCT: f64 = 2.0;

/// Default minimum output ratio (95% of expected)
pub const DEFAULT_MIN_OUTPUT_RATIO: f64 = 0.95;

#[derive(Error, Debug, Clone)]
pub enum QuoteError {
    #[error("Price impact {0:.2}% exceeds maximum {1:.2}%")]
    PriceImpactTooHigh(f64, f64),

    #[error("Quote deviates {0:.2}% from reference price (max {1:.2}%)")]
    OracleDeviationTooHigh(f64, f64),

    #[error("Output ratio {0:.2} below minimum {1:.2} ({2:.1}% slippage)")]
    OutputRatioTooLow(f64, f64, f64),

    #[error("Invalid quote: {0}")]
    InvalidQuote(String),

    #[error("Missing reference price for validation")]
    MissingReferencePrice,

    #[error("Zero or negative amounts in quote")]
    InvalidAmounts,
}

/// Quote information from Jupiter or other DEX aggregator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteInfo {
    /// Input token mint address
    pub input_mint: String,
    /// Output token mint address
    pub output_mint: String,
    /// Input amount in base units (lamports for SOL, etc.)
    pub in_amount: u64,
    /// Output amount in base units
    pub out_amount: u64,
    /// Price impact percentage reported by the DEX
    pub price_impact_pct: f64,
    /// Input token decimals
    pub input_decimals: u8,
    /// Output token decimals
    pub output_decimals: u8,
    /// Slippage tolerance in basis points
    pub slippage_bps: u16,
    /// Route description (optional)
    pub route_info: Option<String>,
}

impl QuoteInfo {
    /// Calculate the effective price (output per input, adjusted for decimals)
    pub fn effective_price(&self) -> f64 {
        if self.in_amount == 0 {
            return 0.0;
        }

        let in_adjusted = self.in_amount as f64 / 10_f64.powi(self.input_decimals as i32);
        let out_adjusted = self.out_amount as f64 / 10_f64.powi(self.output_decimals as i32);

        out_adjusted / in_adjusted
    }

    /// Calculate the inverse effective price (input per output)
    pub fn inverse_effective_price(&self) -> f64 {
        if self.out_amount == 0 {
            return 0.0;
        }

        let in_adjusted = self.in_amount as f64 / 10_f64.powi(self.input_decimals as i32);
        let out_adjusted = self.out_amount as f64 / 10_f64.powi(self.output_decimals as i32);

        in_adjusted / out_adjusted
    }
}

/// Result of quote validation with details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteValidationResult {
    /// Whether the quote is valid
    pub is_valid: bool,
    /// Effective price from the quote
    pub effective_price: f64,
    /// Price impact percentage
    pub price_impact_pct: f64,
    /// Deviation from reference price (if provided)
    pub reference_deviation_pct: Option<f64>,
    /// Output ratio vs expected
    pub output_ratio: Option<f64>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

/// Configuration for quote validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteValidator {
    /// Maximum allowed price impact percentage
    pub max_price_impact_pct: f64,
    /// Maximum allowed deviation from oracle/reference price
    pub max_oracle_deviation_pct: f64,
    /// Minimum acceptable output ratio (actual/expected)
    pub min_output_ratio: f64,
    /// Whether to enforce reference price validation
    pub require_reference_price: bool,
}

impl Default for QuoteValidator {
    fn default() -> Self {
        Self {
            max_price_impact_pct: DEFAULT_MAX_PRICE_IMPACT_PCT,
            max_oracle_deviation_pct: DEFAULT_MAX_ORACLE_DEVIATION_PCT,
            min_output_ratio: DEFAULT_MIN_OUTPUT_RATIO,
            require_reference_price: false,
        }
    }
}

impl QuoteValidator {
    /// Create a new quote validator with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict validator with tighter limits
    pub fn strict() -> Self {
        Self {
            max_price_impact_pct: 1.0,
            max_oracle_deviation_pct: 1.0,
            min_output_ratio: 0.98,
            require_reference_price: true,
        }
    }

    /// Create a lenient validator for lower liquidity tokens
    pub fn lenient() -> Self {
        Self {
            max_price_impact_pct: 5.0,
            max_oracle_deviation_pct: 5.0,
            min_output_ratio: 0.90,
            require_reference_price: false,
        }
    }

    /// Create a custom validator
    pub fn with_config(
        max_price_impact_pct: f64,
        max_oracle_deviation_pct: f64,
        min_output_ratio: f64,
    ) -> Self {
        Self {
            max_price_impact_pct,
            max_oracle_deviation_pct,
            min_output_ratio,
            require_reference_price: false,
        }
    }

    /// Validate a quote against thresholds and optional reference price
    pub fn validate_quote(
        &self,
        quote: &QuoteInfo,
        reference_price: Option<f64>,
    ) -> Result<(), QuoteError> {
        // Basic validation
        if quote.in_amount == 0 || quote.out_amount == 0 {
            return Err(QuoteError::InvalidAmounts);
        }

        // Check price impact
        if quote.price_impact_pct > self.max_price_impact_pct {
            return Err(QuoteError::PriceImpactTooHigh(
                quote.price_impact_pct,
                self.max_price_impact_pct,
            ));
        }

        // Check reference price deviation if provided
        if let Some(ref_price) = reference_price {
            if ref_price > 0.0 {
                let effective = quote.effective_price();
                let deviation_pct = ((effective - ref_price) / ref_price).abs() * 100.0;

                if deviation_pct > self.max_oracle_deviation_pct {
                    return Err(QuoteError::OracleDeviationTooHigh(
                        deviation_pct,
                        self.max_oracle_deviation_pct,
                    ));
                }
            }
        } else if self.require_reference_price {
            return Err(QuoteError::MissingReferencePrice);
        }

        Ok(())
    }

    /// Validate quote with expected output check
    pub fn validate_quote_with_expected(
        &self,
        quote: &QuoteInfo,
        reference_price: Option<f64>,
        expected_output: u64,
    ) -> Result<(), QuoteError> {
        // First run basic validation
        self.validate_quote(quote, reference_price)?;

        // Check output ratio
        if expected_output > 0 {
            let ratio = quote.out_amount as f64 / expected_output as f64;
            if ratio < self.min_output_ratio {
                let slippage_pct = (1.0 - ratio) * 100.0;
                return Err(QuoteError::OutputRatioTooLow(
                    ratio,
                    self.min_output_ratio,
                    slippage_pct,
                ));
            }
        }

        Ok(())
    }

    /// Perform comprehensive validation and return detailed result
    pub fn validate_quote_detailed(
        &self,
        quote: &QuoteInfo,
        reference_price: Option<f64>,
        expected_output: Option<u64>,
    ) -> QuoteValidationResult {
        let mut warnings = Vec::new();
        let mut is_valid = true;

        let effective_price = quote.effective_price();

        // Check price impact
        if quote.price_impact_pct > self.max_price_impact_pct {
            is_valid = false;
        } else if quote.price_impact_pct > self.max_price_impact_pct * 0.75 {
            warnings.push(format!(
                "Price impact {:.2}% approaching limit {:.2}%",
                quote.price_impact_pct, self.max_price_impact_pct
            ));
        }

        // Check reference deviation
        let reference_deviation_pct = reference_price.filter(|&p| p > 0.0).map(|ref_price| {
            let deviation = ((effective_price - ref_price) / ref_price).abs() * 100.0;
            if deviation > self.max_oracle_deviation_pct {
                is_valid = false;
            } else if deviation > self.max_oracle_deviation_pct * 0.75 {
                warnings.push(format!(
                    "Reference deviation {:.2}% approaching limit {:.2}%",
                    deviation, self.max_oracle_deviation_pct
                ));
            }
            deviation
        });

        // Check output ratio
        let output_ratio = expected_output.filter(|&e| e > 0).map(|expected| {
            let ratio = quote.out_amount as f64 / expected as f64;
            if ratio < self.min_output_ratio {
                is_valid = false;
            } else if ratio < self.min_output_ratio * 1.02 {
                warnings.push(format!(
                    "Output ratio {:.3} approaching minimum {:.3}",
                    ratio, self.min_output_ratio
                ));
            }
            ratio
        });

        QuoteValidationResult {
            is_valid,
            effective_price,
            price_impact_pct: quote.price_impact_pct,
            reference_deviation_pct,
            output_ratio,
            warnings,
        }
    }

    /// Calculate the effective price from a quote
    pub fn calculate_effective_price(&self, quote: &QuoteInfo) -> f64 {
        quote.effective_price()
    }

    /// Check if a quote has acceptable price impact
    pub fn has_acceptable_price_impact(&self, quote: &QuoteInfo) -> bool {
        quote.price_impact_pct <= self.max_price_impact_pct
    }

    /// Calculate minimum acceptable output for given expected output
    pub fn minimum_acceptable_output(&self, expected_output: u64) -> u64 {
        (expected_output as f64 * self.min_output_ratio) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_quote() -> QuoteInfo {
        QuoteInfo {
            input_mint: "So11111111111111111111111111111111111111112".to_string(),
            output_mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            in_amount: 1_000_000_000, // 1 SOL (9 decimals)
            out_amount: 100_000_000,   // 100 USDC (6 decimals)
            price_impact_pct: 0.5,
            input_decimals: 9,
            output_decimals: 6,
            slippage_bps: 50,
            route_info: Some("SOL -> USDC via Raydium".to_string()),
        }
    }

    #[test]
    fn test_effective_price() {
        let quote = create_test_quote();
        // 1 SOL = 100 USDC
        let price = quote.effective_price();
        assert!((price - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_inverse_effective_price() {
        let quote = create_test_quote();
        // 1 USDC = 0.01 SOL
        let price = quote.inverse_effective_price();
        assert!((price - 0.01).abs() < 0.0001);
    }

    #[test]
    fn test_valid_quote() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();

        let result = validator.validate_quote(&quote, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_price_impact_too_high() {
        let validator = QuoteValidator::new();
        let mut quote = create_test_quote();
        quote.price_impact_pct = 3.0; // Above 2% default

        let result = validator.validate_quote(&quote, None);
        assert!(matches!(result, Err(QuoteError::PriceImpactTooHigh(_, _))));
    }

    #[test]
    fn test_oracle_deviation_too_high() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();
        let reference_price = 110.0; // Quote gives 100, reference is 110 (9% deviation)

        let result = validator.validate_quote(&quote, Some(reference_price));
        assert!(matches!(result, Err(QuoteError::OracleDeviationTooHigh(_, _))));
    }

    #[test]
    fn test_oracle_deviation_acceptable() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();
        let reference_price = 101.0; // Within 2% deviation

        let result = validator.validate_quote(&quote, Some(reference_price));
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_ratio_too_low() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();
        let expected_output = 120_000_000u64; // Expected 120 USDC, got 100

        let result = validator.validate_quote_with_expected(&quote, None, expected_output);
        assert!(matches!(result, Err(QuoteError::OutputRatioTooLow(_, _, _))));
    }

    #[test]
    fn test_output_ratio_acceptable() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();
        let expected_output = 102_000_000u64; // Expected 102 USDC, got 100 (~98%)

        let result = validator.validate_quote_with_expected(&quote, None, expected_output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_amounts() {
        let validator = QuoteValidator::new();
        let mut quote = create_test_quote();
        quote.in_amount = 0;

        let result = validator.validate_quote(&quote, None);
        assert!(matches!(result, Err(QuoteError::InvalidAmounts)));
    }

    #[test]
    fn test_strict_validator() {
        let validator = QuoteValidator::strict();
        let mut quote = create_test_quote();
        quote.price_impact_pct = 1.5; // Above 1% strict limit

        let result = validator.validate_quote(&quote, None);
        assert!(matches!(result, Err(QuoteError::PriceImpactTooHigh(_, _))));
    }

    #[test]
    fn test_lenient_validator() {
        let validator = QuoteValidator::lenient();
        let mut quote = create_test_quote();
        quote.price_impact_pct = 4.0; // Above default but below lenient

        let result = validator.validate_quote(&quote, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_require_reference_price() {
        let mut validator = QuoteValidator::new();
        validator.require_reference_price = true;
        let quote = create_test_quote();

        let result = validator.validate_quote(&quote, None);
        assert!(matches!(result, Err(QuoteError::MissingReferencePrice)));
    }

    #[test]
    fn test_detailed_validation_valid() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();

        let result = validator.validate_quote_detailed(&quote, Some(100.0), Some(100_000_000));
        assert!(result.is_valid);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_detailed_validation_warnings() {
        let validator = QuoteValidator::new();
        let mut quote = create_test_quote();
        quote.price_impact_pct = 1.8; // 90% of 2% limit

        let result = validator.validate_quote_detailed(&quote, None, None);
        assert!(result.is_valid);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_detailed_validation_invalid() {
        let validator = QuoteValidator::new();
        let mut quote = create_test_quote();
        quote.price_impact_pct = 3.0;

        let result = validator.validate_quote_detailed(&quote, None, None);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_has_acceptable_price_impact() {
        let validator = QuoteValidator::new();
        let mut quote = create_test_quote();

        assert!(validator.has_acceptable_price_impact(&quote));

        quote.price_impact_pct = 3.0;
        assert!(!validator.has_acceptable_price_impact(&quote));
    }

    #[test]
    fn test_minimum_acceptable_output() {
        let validator = QuoteValidator::new();
        let expected = 100_000_000u64;

        let min_output = validator.minimum_acceptable_output(expected);
        assert_eq!(min_output, 95_000_000); // 95% of expected
    }

    #[test]
    fn test_calculate_effective_price() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();

        let price = validator.calculate_effective_price(&quote);
        assert!((price - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_custom_config() {
        let validator = QuoteValidator::with_config(3.0, 3.0, 0.90);
        let mut quote = create_test_quote();
        quote.price_impact_pct = 2.5;

        assert!(validator.validate_quote(&quote, None).is_ok());
    }

    #[test]
    fn test_zero_reference_price_ignored() {
        let validator = QuoteValidator::new();
        let quote = create_test_quote();

        // Zero reference price should be ignored
        let result = validator.validate_quote(&quote, Some(0.0));
        assert!(result.is_ok());
    }

    #[test]
    fn test_effective_price_with_different_decimals() {
        let quote = QuoteInfo {
            input_mint: "test".to_string(),
            output_mint: "test2".to_string(),
            in_amount: 1_000_000_000_000, // 1000 tokens (9 decimals)
            out_amount: 500_000_000,       // 500 tokens (6 decimals)
            price_impact_pct: 0.1,
            input_decimals: 9,
            output_decimals: 6,
            slippage_bps: 50,
            route_info: None,
        };

        // 1000 input -> 500 output = 0.5 output per input
        let price = quote.effective_price();
        assert!((price - 0.5).abs() < 0.001);
    }
}
