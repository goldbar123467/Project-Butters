//! Jupiter Quote Types
//!
//! Request and response structures for Jupiter V6 quote API.

use serde::{Deserialize, Serialize};

/// Request parameters for getting a swap quote
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    /// Input token mint address
    pub input_mint: String,
    /// Output token mint address
    pub output_mint: String,
    /// Amount in base units (lamports for SOL)
    pub amount: u64,
    /// Slippage tolerance in basis points (1 = 0.01%)
    pub slippage_bps: u16,
    /// Only use direct routes (no intermediate tokens)
    #[serde(default)]
    pub only_direct_routes: bool,
    /// Restrict to intermediate tokens (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_intermediate_tokens: Option<bool>,
    /// Platform fee in basis points (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_fee_bps: Option<u16>,
}

impl QuoteRequest {
    /// Create a new quote request with required parameters
    pub fn new(input_mint: String, output_mint: String, amount: u64, slippage_bps: u16) -> Self {
        Self {
            input_mint,
            output_mint,
            amount,
            slippage_bps,
            only_direct_routes: false,
            restrict_intermediate_tokens: None,
            platform_fee_bps: None,
        }
    }

    /// Set only direct routes flag
    pub fn with_direct_routes(mut self, direct: bool) -> Self {
        self.only_direct_routes = direct;
        self
    }

    /// Set platform fee
    pub fn with_platform_fee(mut self, fee_bps: u16) -> Self {
        self.platform_fee_bps = Some(fee_bps);
        self
    }
}

/// Response from Jupiter quote API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    /// Input token mint address
    pub input_mint: String,
    /// Output token mint address
    pub output_mint: String,
    /// Input amount in base units
    pub in_amount: String,
    /// Output amount in base units
    pub out_amount: String,
    /// Minimum output amount after slippage (otherAmountThreshold)
    pub other_amount_threshold: String,
    /// Swap mode (ExactIn or ExactOut)
    pub swap_mode: String,
    /// Slippage in basis points
    pub slippage_bps: u16,
    /// Price impact percentage (as string)
    #[serde(default)]
    pub price_impact_pct: String,
    /// Route plan with swap details
    pub route_plan: Vec<RoutePlanStep>,
    /// Context slot for the quote
    #[serde(default)]
    pub context_slot: Option<u64>,
    /// Time taken in milliseconds
    #[serde(default)]
    pub time_taken: Option<f64>,
    /// Catch-all for any additional fields from API (prevents future field loss)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl QuoteResponse {
    /// Get input amount as u64
    pub fn input_amount(&self) -> u64 {
        self.in_amount.parse().unwrap_or(0)
    }

    /// Get output amount as u64
    pub fn output_amount(&self) -> u64 {
        self.out_amount.parse().unwrap_or(0)
    }

    /// Get minimum output amount as u64
    pub fn min_output_amount(&self) -> u64 {
        self.other_amount_threshold.parse().unwrap_or(0)
    }

    /// Get price impact as f64 percentage
    pub fn price_impact(&self) -> f64 {
        self.price_impact_pct.parse().unwrap_or(0.0)
    }

    /// Check if price impact is acceptable (< threshold %)
    pub fn is_price_impact_acceptable(&self, max_impact_pct: f64) -> bool {
        self.price_impact() < max_impact_pct
    }
}

/// A step in the route plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlanStep {
    /// Swap information for this step
    pub swap_info: SwapInfo,
    /// Percentage of the trade going through this route
    pub percent: u8,
}

/// Information about a single swap in the route
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    /// AMM key (pool identifier)
    pub amm_key: String,
    /// Label for the DEX (e.g., "Raydium", "Orca")
    pub label: String,
    /// Input mint for this hop
    pub input_mint: String,
    /// Output mint for this hop
    pub output_mint: String,
    /// Input amount for this hop
    pub in_amount: String,
    /// Output amount for this hop
    pub out_amount: String,
    /// Fee amount charged (optional - not always returned by Jupiter API as of 2026-01-06)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_amount: Option<String>,
    /// Fee mint token (optional - not always returned by Jupiter API as of 2026-01-06)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_mint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_request_new() {
        let req = QuoteRequest::new(
            "So11111111111111111111111111111111111111112".to_string(),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            1_000_000_000, // 1 SOL
            50,            // 0.5%
        );

        assert_eq!(req.amount, 1_000_000_000);
        assert_eq!(req.slippage_bps, 50);
        assert!(!req.only_direct_routes);
    }

    #[test]
    fn test_quote_request_builder() {
        let req = QuoteRequest::new(
            "SOL".to_string(),
            "USDC".to_string(),
            1_000_000,
            100,
        )
        .with_direct_routes(true)
        .with_platform_fee(10);

        assert!(req.only_direct_routes);
        assert_eq!(req.platform_fee_bps, Some(10));
    }

    #[test]
    fn test_quote_response_parsing() {
        let json = r#"{
            "inputMint": "So11111111111111111111111111111111111111112",
            "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "inAmount": "1000000000",
            "outAmount": "150000000",
            "otherAmountThreshold": "149250000",
            "swapMode": "ExactIn",
            "slippageBps": 50,
            "priceImpactPct": "0.12",
            "routePlan": [{
                "swapInfo": {
                    "ammKey": "pool123",
                    "label": "Raydium",
                    "inputMint": "SOL",
                    "outputMint": "USDC",
                    "inAmount": "1000000000",
                    "outAmount": "150000000",
                    "feeAmount": "1500",
                    "feeMint": "USDC"
                },
                "percent": 100
            }]
        }"#;

        let quote: QuoteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(quote.input_amount(), 1_000_000_000);
        assert_eq!(quote.output_amount(), 150_000_000);
        assert_eq!(quote.min_output_amount(), 149_250_000);
        assert!((quote.price_impact() - 0.12).abs() < 0.001);
        assert!(quote.is_price_impact_acceptable(1.0));
    }

    #[test]
    fn test_route_plan_parsing() {
        let json = r#"{
            "swapInfo": {
                "ammKey": "pool123",
                "label": "Orca",
                "inputMint": "SOL",
                "outputMint": "USDC",
                "inAmount": "500000000",
                "outAmount": "75000000",
                "feeAmount": "750",
                "feeMint": "USDC"
            },
            "percent": 50
        }"#;

        let step: RoutePlanStep = serde_json::from_str(json).unwrap();
        assert_eq!(step.percent, 50);
        assert_eq!(step.swap_info.label, "Orca");
    }
}
