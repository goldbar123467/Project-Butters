//! Pump.fun Bonding Curve Types
//!
//! Types for interacting with pump.fun's bonding curve mechanism.
//! Pump.fun tokens trade on a bonding curve until graduation (~85 SOL / $69K market cap),
//! at which point they migrate to Raydium with initial liquidity.
//!
//! Bonding Curve Formula:
//! - Price = virtual_sol_reserves / virtual_token_reserves
//! - Market Cap = price * (total_supply - remaining_tokens)
//!
//! Key Constants (pump.fun defaults):
//! - Total supply: 1,000,000,000 tokens
//! - Initial virtual SOL: 30 SOL
//! - Initial virtual tokens: 1,073,000,000 (1.073B)
//! - Graduation threshold: ~85 SOL in reserves

use serde::{Deserialize, Serialize};

/// SOL decimals (9)
pub const SOL_DECIMALS: u8 = 9;

/// Pump.fun token decimals (6)
pub const PUMP_TOKEN_DECIMALS: u8 = 6;

/// Total token supply for pump.fun tokens (1 billion)
pub const PUMP_TOTAL_SUPPLY: u64 = 1_000_000_000_000_000; // 1B with 6 decimals

/// Initial virtual SOL reserves (30 SOL in lamports)
pub const INITIAL_VIRTUAL_SOL: u64 = 30_000_000_000; // 30 SOL

/// Initial virtual token reserves (1.073B tokens)
pub const INITIAL_VIRTUAL_TOKENS: u64 = 1_073_000_000_000_000; // 1.073B with 6 decimals

/// Graduation threshold - approximately 85 SOL
pub const GRADUATION_SOL_THRESHOLD: u64 = 85_000_000_000; // 85 SOL in lamports

/// Tokens available on bonding curve (800M)
pub const BONDING_CURVE_TOKENS: u64 = 800_000_000_000_000; // 800M with 6 decimals

/// Bonding curve state from pump.fun
///
/// This represents the current state of a token's bonding curve.
/// The price is determined by: virtual_sol_reserves / virtual_token_reserves
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BondingCurveState {
    /// Token mint address
    pub mint: String,
    /// Virtual SOL reserves in lamports (includes initial virtual liquidity)
    pub virtual_sol_reserves: u64,
    /// Virtual token reserves (includes initial virtual tokens)
    pub virtual_token_reserves: u64,
    /// Real SOL reserves (actual SOL in the curve)
    pub real_sol_reserves: u64,
    /// Real token reserves (actual tokens remaining to be sold)
    pub real_token_reserves: u64,
    /// Whether the bonding curve is complete (graduated to DEX)
    pub complete: bool,
    /// Total supply of the token
    pub total_supply: u64,
}

impl Default for BondingCurveState {
    fn default() -> Self {
        Self {
            mint: String::new(),
            virtual_sol_reserves: INITIAL_VIRTUAL_SOL,
            virtual_token_reserves: INITIAL_VIRTUAL_TOKENS,
            real_sol_reserves: 0,
            real_token_reserves: BONDING_CURVE_TOKENS,
            complete: false,
            total_supply: PUMP_TOTAL_SUPPLY,
        }
    }
}

impl BondingCurveState {
    /// Create a new bonding curve state
    pub fn new(mint: String) -> Self {
        Self {
            mint,
            ..Default::default()
        }
    }

    /// Create from on-chain data
    pub fn from_reserves(
        mint: String,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
        real_sol_reserves: u64,
        real_token_reserves: u64,
        complete: bool,
    ) -> Self {
        Self {
            mint,
            virtual_sol_reserves,
            virtual_token_reserves,
            real_sol_reserves,
            real_token_reserves,
            complete,
            total_supply: PUMP_TOTAL_SUPPLY,
        }
    }

    /// Calculate current price per token in SOL
    ///
    /// Formula: virtual_sol_reserves / virtual_token_reserves
    /// Returns price for 1 whole token (adjusted for decimals)
    pub fn price_per_token(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }

        // Price = (virtual_sol / 10^9) / (virtual_tokens / 10^6)
        // Simplifies to: virtual_sol * 10^6 / (virtual_tokens * 10^9)
        // = virtual_sol / (virtual_tokens * 10^3)
        let sol = self.virtual_sol_reserves as f64 / 1e9; // Convert lamports to SOL
        let tokens = self.virtual_token_reserves as f64 / 1e6; // Convert to whole tokens

        sol / tokens
    }

    /// Calculate price per token in lamports per token base unit
    /// This is the raw price without decimal adjustment
    pub fn price_lamports_per_base(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }

        self.virtual_sol_reserves as f64 / self.virtual_token_reserves as f64
    }

    /// Calculate market cap in SOL
    ///
    /// Market cap = price * circulating supply
    /// Circulating supply = total_supply - real_token_reserves
    pub fn market_cap_sol(&self) -> f64 {
        let price = self.price_per_token();
        let circulating = (self.total_supply - self.real_token_reserves) as f64 / 1e6;
        price * circulating
    }

    /// Calculate graduation progress as a percentage (0-100)
    ///
    /// Based on real SOL reserves relative to graduation threshold
    pub fn graduation_progress(&self) -> f64 {
        if self.complete {
            return 100.0;
        }

        let progress = (self.real_sol_reserves as f64 / GRADUATION_SOL_THRESHOLD as f64) * 100.0;
        progress.min(100.0)
    }

    /// Check if the token is close to graduation (>80%)
    pub fn is_near_graduation(&self) -> bool {
        self.graduation_progress() >= 80.0
    }

    /// Estimate bonding curve state from market cap in SOL
    ///
    /// This is useful when we only know the market cap from external sources
    /// and need to estimate the bonding curve state.
    ///
    /// Returns None if the market cap is invalid or beyond graduation
    pub fn estimate_from_market_cap(mint: String, market_cap_sol: f64) -> Option<Self> {
        if market_cap_sol <= 0.0 {
            return None;
        }

        // Max market cap before graduation is approximately:
        // At graduation: ~85 SOL real reserves, price ~ 0.000028 SOL/token
        // Circulating: ~200M tokens, so market cap ~ 5600 SOL ($500K+)
        // If market cap > reasonable threshold, assume graduated
        if market_cap_sol > 10000.0 {
            return None; // Likely graduated, use Jupiter
        }

        // Estimate the price and reserves
        // This is an approximation - real state comes from WebSocket
        let estimated_price = market_cap_sol / 200_000_000.0; // Assume ~200M circulating

        // Back-calculate virtual reserves
        // price = virtual_sol / virtual_tokens
        // Keep virtual_tokens roughly at initial, adjust virtual_sol
        let virtual_tokens = INITIAL_VIRTUAL_TOKENS;
        let virtual_sol = (estimated_price * (virtual_tokens as f64 / 1e6) * 1e9) as u64;

        // Estimate real reserves
        let real_sol = if market_cap_sol < 100.0 {
            (market_cap_sol * 0.1 * 1e9) as u64 // Very rough estimate
        } else {
            ((market_cap_sol / 100.0) * 1e9) as u64
        };

        Some(Self {
            mint,
            virtual_sol_reserves: virtual_sol.max(INITIAL_VIRTUAL_SOL),
            virtual_token_reserves: virtual_tokens,
            real_sol_reserves: real_sol.min(GRADUATION_SOL_THRESHOLD),
            real_token_reserves: BONDING_CURVE_TOKENS - (real_sol / 1000), // Rough token sale estimate
            complete: false,
            total_supply: PUMP_TOTAL_SUPPLY,
        })
    }

    /// Update state from trade event data
    pub fn update_from_trade(
        &mut self,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
        is_complete: bool,
    ) {
        // Calculate real reserves from virtual (subtract initial)
        self.real_sol_reserves = virtual_sol_reserves.saturating_sub(INITIAL_VIRTUAL_SOL);
        self.real_token_reserves = BONDING_CURVE_TOKENS
            .saturating_sub(INITIAL_VIRTUAL_TOKENS.saturating_sub(virtual_token_reserves));

        self.virtual_sol_reserves = virtual_sol_reserves;
        self.virtual_token_reserves = virtual_token_reserves;
        self.complete = is_complete;
    }

    /// Calculate how much SOL is needed to buy a given amount of tokens
    pub fn calculate_buy_cost(&self, token_amount: u64) -> u64 {
        if self.virtual_token_reserves <= token_amount {
            return u64::MAX; // Not enough tokens available
        }

        // Using constant product formula: x * y = k
        // new_token_reserves = virtual_token_reserves - token_amount
        // new_sol_reserves = k / new_token_reserves
        // cost = new_sol_reserves - virtual_sol_reserves

        let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
        let new_token_reserves = self.virtual_token_reserves - token_amount;
        let new_sol_reserves = (k / new_token_reserves as u128) as u64;

        new_sol_reserves.saturating_sub(self.virtual_sol_reserves)
    }

    /// Calculate how much SOL is received for selling a given amount of tokens
    pub fn calculate_sell_return(&self, token_amount: u64) -> u64 {
        // Using constant product formula: x * y = k
        // new_token_reserves = virtual_token_reserves + token_amount
        // new_sol_reserves = k / new_token_reserves
        // return = virtual_sol_reserves - new_sol_reserves

        let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
        let new_token_reserves = self.virtual_token_reserves + token_amount;
        let new_sol_reserves = (k / new_token_reserves as u128) as u64;

        self.virtual_sol_reserves.saturating_sub(new_sol_reserves)
    }
}

/// Pump.fun token state tracked by the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunTokenState {
    /// Bonding curve state
    pub bonding_curve: BondingCurveState,
    /// Current price in SOL
    pub price_sol: f64,
    /// Last trade timestamp (Unix seconds)
    pub last_trade_ts: u64,
    /// Last update timestamp
    pub last_update_ts: u64,
}

impl PumpFunTokenState {
    /// Create a new pump.fun token state
    pub fn new(bonding_curve: BondingCurveState) -> Self {
        let price_sol = bonding_curve.price_per_token();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            bonding_curve,
            price_sol,
            last_trade_ts: now,
            last_update_ts: now,
        }
    }

    /// Update from trade event
    pub fn update_from_trade(
        &mut self,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
        is_complete: bool,
        trade_ts: u64,
    ) {
        self.bonding_curve
            .update_from_trade(virtual_sol_reserves, virtual_token_reserves, is_complete);
        self.price_sol = self.bonding_curve.price_per_token();
        self.last_trade_ts = trade_ts;
        self.last_update_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Check if the token is still on the bonding curve
    pub fn is_on_bonding_curve(&self) -> bool {
        !self.bonding_curve.complete
    }

    /// Get price in USDC given SOL/USDC price
    pub fn price_usdc(&self, sol_usdc_price: f64) -> f64 {
        self.price_sol * sol_usdc_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bonding_curve_default() {
        let bc = BondingCurveState::default();
        assert_eq!(bc.virtual_sol_reserves, INITIAL_VIRTUAL_SOL);
        assert_eq!(bc.virtual_token_reserves, INITIAL_VIRTUAL_TOKENS);
        assert!(!bc.complete);
    }

    #[test]
    fn test_price_per_token_initial() {
        let bc = BondingCurveState::default();
        let price = bc.price_per_token();

        // Initial price: 30 SOL / 1.073B tokens = ~0.000000028 SOL/token
        assert!(price > 0.0);
        assert!(price < 0.0001); // Should be a very small number
    }

    #[test]
    fn test_price_per_token_after_buys() {
        let mut bc = BondingCurveState::default();

        // Simulate some trading activity - more SOL, fewer tokens
        bc.virtual_sol_reserves = 50_000_000_000; // 50 SOL
        bc.virtual_token_reserves = 800_000_000_000_000; // 800M tokens

        let price = bc.price_per_token();

        // Price should be higher after buys
        let initial = BondingCurveState::default();
        assert!(price > initial.price_per_token());
    }

    #[test]
    fn test_graduation_progress() {
        let mut bc = BondingCurveState::default();
        assert_eq!(bc.graduation_progress(), 0.0);

        // 50% progress
        bc.real_sol_reserves = GRADUATION_SOL_THRESHOLD / 2;
        assert!((bc.graduation_progress() - 50.0).abs() < 0.1);

        // Complete
        bc.complete = true;
        assert_eq!(bc.graduation_progress(), 100.0);
    }

    #[test]
    fn test_market_cap_sol() {
        let bc = BondingCurveState::default();
        let mcap = bc.market_cap_sol();

        // With initial state, market cap should be positive but small
        // Circulating = 1B - 800M = 200M tokens
        // Price ~ 0.000000028 SOL
        // MCap ~ 200M * 0.000000028 ~ 5.6 SOL
        assert!(mcap > 0.0);
        assert!(mcap < 100.0);
    }

    #[test]
    fn test_estimate_from_market_cap() {
        let state = BondingCurveState::estimate_from_market_cap("test".to_string(), 100.0);
        assert!(state.is_some());

        let state = state.unwrap();
        assert!(!state.complete);
        assert!(state.virtual_sol_reserves >= INITIAL_VIRTUAL_SOL);

        // Very high market cap should return None (graduated)
        let graduated = BondingCurveState::estimate_from_market_cap("test".to_string(), 50000.0);
        assert!(graduated.is_none());
    }

    #[test]
    fn test_is_near_graduation() {
        let mut bc = BondingCurveState::default();
        assert!(!bc.is_near_graduation());

        bc.real_sol_reserves = (GRADUATION_SOL_THRESHOLD as f64 * 0.85) as u64;
        assert!(bc.is_near_graduation());
    }

    #[test]
    fn test_calculate_buy_cost() {
        let bc = BondingCurveState::default();

        // Buy a small amount
        let cost = bc.calculate_buy_cost(1_000_000); // 1 token (with 6 decimals)
        assert!(cost > 0);

        // Buying more should cost more
        let cost_10 = bc.calculate_buy_cost(10_000_000);
        assert!(cost_10 > cost);
    }

    #[test]
    fn test_calculate_sell_return() {
        let mut bc = BondingCurveState::default();
        bc.virtual_sol_reserves = 50_000_000_000; // More SOL after some buys

        let return_amount = bc.calculate_sell_return(1_000_000);
        assert!(return_amount > 0);
    }

    #[test]
    fn test_pump_fun_token_state() {
        let bc = BondingCurveState::new("test_mint".to_string());
        let state = PumpFunTokenState::new(bc);

        assert!(state.is_on_bonding_curve());
        assert!(state.price_sol > 0.0);
    }

    #[test]
    fn test_pump_fun_price_usdc() {
        let bc = BondingCurveState::new("test".to_string());
        let state = PumpFunTokenState::new(bc);

        let sol_price = 200.0; // $200/SOL
        let usdc_price = state.price_usdc(sol_price);

        assert!(usdc_price > 0.0);
        assert!((usdc_price / state.price_sol - sol_price).abs() < 0.0001);
    }

    #[test]
    fn test_update_from_trade() {
        let bc = BondingCurveState::new("test".to_string());
        let mut state = PumpFunTokenState::new(bc);

        let old_price = state.price_sol;

        // Simulate buy (more SOL, fewer tokens)
        state.update_from_trade(
            50_000_000_000,      // 50 SOL
            800_000_000_000_000, // 800M tokens
            false,
            12345,
        );

        assert!(state.price_sol > old_price);
        assert_eq!(state.last_trade_ts, 12345);
    }
}
