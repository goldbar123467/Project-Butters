//! Sell Simulation via Jupiter
//!
//! Simulates sell transactions using Jupiter quotes to verify
//! that a token can actually be sold.

use crate::adapters::jupiter::{JupiterClient, QuoteRequest};
use crate::domain::honeypot_detector::{HoneypotError, SimulationResult};
use crate::ports::execution::ExecutionError;
use solana_sdk::pubkey::Pubkey;

/// USDC mint address on Solana mainnet
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// SOL native mint (wrapped SOL)
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Sell simulator using Jupiter quotes
pub struct SellSimulator {
    jupiter: JupiterClient,
    /// Default slippage in basis points
    slippage_bps: u16,
    /// Output mint for sell simulation (default: USDC)
    output_mint: String,
}

impl SellSimulator {
    /// Create a new sell simulator
    pub fn new(jupiter: JupiterClient) -> Self {
        Self {
            jupiter,
            slippage_bps: 500, // 5% slippage tolerance for simulation
            output_mint: USDC_MINT.to_string(),
        }
    }

    /// Create a new sell simulator with custom settings
    pub fn with_config(jupiter: JupiterClient, slippage_bps: u16, output_mint: String) -> Self {
        Self {
            jupiter,
            slippage_bps,
            output_mint,
        }
    }

    /// Simulate selling a token
    ///
    /// # Arguments
    /// * `mint` - Token mint address to sell
    /// * `amount` - Amount of tokens to sell (in base units)
    ///
    /// # Returns
    /// * `SimulationResult` - Success/failure with estimated output
    pub async fn simulate_sell(
        &self,
        mint: &Pubkey,
        amount: u64,
    ) -> Result<SimulationResult, HoneypotError> {
        // Can't simulate with zero amount
        if amount == 0 {
            return Ok(SimulationResult::failed("Cannot simulate with zero amount"));
        }

        let quote_request = QuoteRequest {
            input_mint: mint.to_string(),
            output_mint: self.output_mint.clone(),
            amount,
            slippage_bps: self.slippage_bps,
            only_direct_routes: false,
            restrict_intermediate_tokens: None,
            platform_fee_bps: None,
        };

        match self.jupiter.get_quote(&quote_request).await {
            Ok(quote) => {
                // Parse output amounts
                let out_amount: u64 = quote.out_amount.parse().unwrap_or(0);
                let in_amount: u64 = quote.in_amount.parse().unwrap_or(amount);

                // Calculate implicit fee (difference between input and effective output)
                // This is a rough estimate since we're converting token -> USDC
                let estimated_fees = if out_amount > 0 && in_amount > 0 {
                    // Rough fee calculation based on route info
                    let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);
                    let fee_ratio = price_impact.abs() / 100.0;
                    (in_amount as f64 * fee_ratio) as u64
                } else {
                    0
                };

                // Check for excessive price impact (>25% indicates likely honeypot)
                let price_impact = quote.price_impact_pct.parse::<f64>().unwrap_or(0.0);
                if price_impact.abs() > 25.0 {
                    return Ok(SimulationResult::failed(format!(
                        "Excessive price impact: {:.2}% - likely honeypot or extremely illiquid",
                        price_impact
                    )));
                }

                // Check that we got a non-zero output
                if out_amount == 0 {
                    return Ok(SimulationResult::failed("Quote returned zero output amount"));
                }

                Ok(SimulationResult::success(out_amount, estimated_fees))
            }
            Err(e) => {
                // Check for specific error patterns
                let error_str = e.to_string().to_lowercase();

                if error_str.contains("no routes")
                    || error_str.contains("no route found")
                    || error_str.contains("could not find any routes")
                {
                    return Ok(SimulationResult::failed(
                        "No liquidity routes found - token may be untradeable",
                    ));
                }

                if error_str.contains("insufficient")
                    || error_str.contains("not enough")
                {
                    return Ok(SimulationResult::failed(
                        "Insufficient liquidity for sell amount",
                    ));
                }

                // Rate limiting is not a token issue
                if error_str.contains("rate limit") || error_str.contains("429") {
                    return Err(HoneypotError::SimulationFailed {
                        reason: "Jupiter API rate limited - try again later".to_string(),
                    });
                }

                // Generic API error
                Ok(SimulationResult::failed(format!(
                    "Jupiter quote failed: {}",
                    e
                )))
            }
        }
    }

    /// Simulate selling with different amounts to detect size-based restrictions
    ///
    /// Some honeypots allow small sells but block large ones.
    /// This method tests multiple sizes to detect this pattern.
    pub async fn simulate_sell_multi_size(
        &self,
        mint: &Pubkey,
        amounts: &[u64],
    ) -> Result<Vec<(u64, SimulationResult)>, HoneypotError> {
        let mut results = Vec::new();

        for &amount in amounts {
            let result = self.simulate_sell(mint, amount).await?;
            results.push((amount, result));

            // If small amount fails, no point testing larger amounts
            if !results.last().map(|(_, r)| r.success).unwrap_or(false) {
                break;
            }
        }

        Ok(results)
    }

    /// Quick sell check - just verifies a route exists
    pub async fn can_sell(&self, mint: &Pubkey, amount: u64) -> Result<bool, HoneypotError> {
        let result = self.simulate_sell(mint, amount).await?;
        Ok(result.success)
    }

    /// Get the output mint used for simulations
    pub fn output_mint(&self) -> &str {
        &self.output_mint
    }

    /// Set the output mint for simulations
    pub fn set_output_mint(&mut self, output_mint: String) {
        self.output_mint = output_mint;
    }

    /// Set slippage tolerance
    pub fn set_slippage_bps(&mut self, slippage_bps: u16) {
        self.slippage_bps = slippage_bps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sell_simulator_creation() {
        let jupiter = JupiterClient::new().unwrap();
        let simulator = SellSimulator::new(jupiter);

        assert_eq!(simulator.output_mint(), USDC_MINT);
        assert_eq!(simulator.slippage_bps, 500);
    }

    #[test]
    fn test_sell_simulator_with_config() {
        let jupiter = JupiterClient::new().unwrap();
        let simulator = SellSimulator::with_config(jupiter, 100, WSOL_MINT.to_string());

        assert_eq!(simulator.output_mint(), WSOL_MINT);
        assert_eq!(simulator.slippage_bps, 100);
    }

    #[test]
    fn test_set_output_mint() {
        let jupiter = JupiterClient::new().unwrap();
        let mut simulator = SellSimulator::new(jupiter);

        simulator.set_output_mint(WSOL_MINT.to_string());
        assert_eq!(simulator.output_mint(), WSOL_MINT);
    }

    #[test]
    fn test_set_slippage() {
        let jupiter = JupiterClient::new().unwrap();
        let mut simulator = SellSimulator::new(jupiter);

        simulator.set_slippage_bps(1000);
        assert_eq!(simulator.slippage_bps, 1000);
    }

    // Note: Integration tests for actual simulation would require
    // network access and are better suited for the integration test suite
}
