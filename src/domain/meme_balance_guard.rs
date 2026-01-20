//! Meme Balance Guard
//!
//! Extended balance guard for meme coin trading with relaxed thresholds.
//! Wraps the existing BalanceGuard with meme-specific configuration.
//!
//! Key differences from standard BalanceGuard:
//! - Higher variance tolerance (0.01 SOL vs 0.001 SOL per trade)
//! - Higher cumulative threshold (0.1 SOL vs 0.01 SOL)
//! - Configurable slippage awareness for meme volatility
//! - Token balance tracking support

use super::balance_guard::{
    BalanceGuard, BalanceGuardConfig, BalanceGuardError,
    BalanceViolation, ExpectedDelta, GuardStatus,
};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Default threshold for meme trading: 0.01 SOL (10x standard)
pub const DEFAULT_MEME_THRESHOLD_LAMPORTS: u64 = 10_000_000;

/// Default cumulative threshold for meme trading: 0.1 SOL (10x standard)
pub const DEFAULT_MEME_CUMULATIVE_THRESHOLD: i64 = 100_000_000;

/// Default max slippage in basis points (5%)
pub const DEFAULT_MAX_SLIPPAGE_BPS: u16 = 500;

/// Errors specific to meme balance guard
#[derive(Error, Debug, Clone)]
pub enum MemeBalanceGuardError {
    #[error("Underlying balance guard error: {0}")]
    BalanceGuard(#[from] BalanceGuardError),

    #[error("Token balance anomaly: mint {mint}, expected {expected}, actual {actual}")]
    TokenBalanceAnomaly {
        mint: String,
        expected: u64,
        actual: u64,
    },

    #[error("Slippage exceeded: max {max_bps} bps, actual {actual_bps} bps")]
    SlippageExceeded { max_bps: u16, actual_bps: u16 },

    #[error("No token snapshot for mint: {0}")]
    NoTokenSnapshot(String),
}

/// Configuration for meme balance guard
#[derive(Debug, Clone)]
pub struct MemeBalanceGuardConfig {
    /// Base balance guard config (for SOL tracking)
    pub base_config: BalanceGuardConfig,

    /// Maximum expected slippage in basis points (e.g., 500 = 5%)
    pub max_slippage_bps: u16,

    /// Whether to track token balances in addition to SOL
    pub token_tracking_enabled: bool,

    /// Tolerance percentage for token balance validation (e.g., 0.10 = 10%)
    pub token_tolerance_pct: f64,
}

impl Default for MemeBalanceGuardConfig {
    fn default() -> Self {
        Self {
            base_config: BalanceGuardConfig {
                threshold_lamports: DEFAULT_MEME_THRESHOLD_LAMPORTS,
                cumulative_threshold: DEFAULT_MEME_CUMULATIVE_THRESHOLD,
                halt_on_violation: true,
            },
            max_slippage_bps: DEFAULT_MAX_SLIPPAGE_BPS,
            token_tracking_enabled: true,
            token_tolerance_pct: 0.10, // 10% tolerance for token amounts
        }
    }
}

impl MemeBalanceGuardConfig {
    /// Create a conservative config for graduated tokens with more liquidity
    pub fn graduated() -> Self {
        Self {
            base_config: BalanceGuardConfig {
                threshold_lamports: 5_000_000, // 0.005 SOL
                cumulative_threshold: 50_000_000, // 0.05 SOL
                halt_on_violation: true,
            },
            max_slippage_bps: 300, // 3%
            token_tracking_enabled: true,
            token_tolerance_pct: 0.05, // 5% tolerance
        }
    }

    /// Create a lenient config for pump.fun sniping (high volatility)
    pub fn pump_fun_sniper() -> Self {
        Self {
            base_config: BalanceGuardConfig {
                threshold_lamports: 20_000_000, // 0.02 SOL
                cumulative_threshold: 200_000_000, // 0.2 SOL
                halt_on_violation: true,
            },
            max_slippage_bps: 1000, // 10%
            token_tracking_enabled: true,
            token_tolerance_pct: 0.15, // 15% tolerance
        }
    }

    /// Create config with custom thresholds
    pub fn custom(
        threshold_lamports: u64,
        cumulative_threshold: i64,
        max_slippage_bps: u16,
    ) -> Self {
        Self {
            base_config: BalanceGuardConfig {
                threshold_lamports,
                cumulative_threshold,
                halt_on_violation: true,
            },
            max_slippage_bps,
            token_tracking_enabled: true,
            token_tolerance_pct: 0.10,
        }
    }
}

/// Token balance snapshot
#[derive(Debug, Clone)]
pub struct TokenSnapshot {
    pub mint: Pubkey,
    pub balance: u64,
    pub timestamp: u64,
}

impl TokenSnapshot {
    pub fn new(mint: Pubkey, balance: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            mint,
            balance,
            timestamp,
        }
    }
}

/// Expected delta with slippage calculation
#[derive(Debug, Clone)]
pub struct MemeExpectedDelta {
    /// Base expected delta
    pub base: ExpectedDelta,
    /// Maximum slippage in basis points
    pub max_slippage_bps: u16,
    /// Expected token amount (if buying tokens)
    pub expected_tokens: Option<u64>,
    /// Token mint (if tracking tokens)
    pub token_mint: Option<Pubkey>,
}

impl MemeExpectedDelta {
    /// Create expected delta for SOL -> Meme Token swap
    pub fn sol_to_meme(
        input_lamports: u64,
        priority_fee: u64,
        jito_tip: u64,
        max_slippage_bps: u16,
        expected_tokens: u64,
        token_mint: Pubkey,
    ) -> Self {
        // Account for slippage in expected SOL spending
        let slippage_factor = 1.0 + (max_slippage_bps as f64 / 10_000.0);
        let worst_case_input = (input_lamports as f64 * slippage_factor) as u64;

        Self {
            base: ExpectedDelta::sol_to_token(worst_case_input, priority_fee, jito_tip),
            max_slippage_bps,
            expected_tokens: Some(expected_tokens),
            token_mint: Some(token_mint),
        }
    }

    /// Create expected delta for Meme Token -> SOL swap
    pub fn meme_to_sol(
        expected_output_lamports: u64,
        priority_fee: u64,
        jito_tip: u64,
        max_slippage_bps: u16,
    ) -> Self {
        // Account for slippage - we might receive less
        let slippage_factor = 1.0 - (max_slippage_bps as f64 / 10_000.0);
        let worst_case_output = (expected_output_lamports as f64 * slippage_factor) as u64;

        Self {
            base: ExpectedDelta::token_to_sol(worst_case_output, priority_fee, jito_tip),
            max_slippage_bps,
            expected_tokens: None,
            token_mint: None,
        }
    }

    /// Create a custom expected delta
    pub fn custom(
        sol_change: i64,
        reason: impl Into<String>,
        max_slippage_bps: u16,
    ) -> Self {
        Self {
            base: ExpectedDelta::custom(sol_change, reason),
            max_slippage_bps,
            expected_tokens: None,
            token_mint: None,
        }
    }
}

/// Meme balance guard wrapping standard BalanceGuard
pub struct MemeBalanceGuard {
    /// Underlying balance guard
    inner: BalanceGuard,
    /// Meme-specific configuration
    config: MemeBalanceGuardConfig,
    /// Token balance snapshots (mint -> snapshot)
    token_snapshots: HashMap<Pubkey, TokenSnapshot>,
    /// Token balance violations
    token_violations: Vec<TokenViolation>,
}

/// Record of a token balance violation
#[derive(Debug, Clone)]
pub struct TokenViolation {
    pub timestamp: u64,
    pub mint: Pubkey,
    pub expected: u64,
    pub actual: u64,
    pub tolerance_pct: f64,
    pub reason: String,
}

impl MemeBalanceGuard {
    /// Create a new meme balance guard
    pub fn new(user_wallet: Pubkey) -> Self {
        Self::with_config(user_wallet, MemeBalanceGuardConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(user_wallet: Pubkey, config: MemeBalanceGuardConfig) -> Self {
        Self {
            inner: BalanceGuard::with_config(user_wallet, config.base_config.clone()),
            config,
            token_snapshots: HashMap::new(),
            token_violations: Vec::new(),
        }
    }

    /// Create with graduated token config
    pub fn graduated(user_wallet: Pubkey) -> Self {
        Self::with_config(user_wallet, MemeBalanceGuardConfig::graduated())
    }

    /// Create with pump.fun sniper config
    pub fn pump_fun_sniper(user_wallet: Pubkey) -> Self {
        Self::with_config(user_wallet, MemeBalanceGuardConfig::pump_fun_sniper())
    }

    /// Check if trading is halted
    pub fn is_halted(&self) -> bool {
        self.inner.is_halted()
    }

    /// Get cumulative unexplained losses
    pub fn cumulative_unexplained(&self) -> i64 {
        self.inner.cumulative_unexplained()
    }

    /// Get SOL violation history
    pub fn sol_violations(&self) -> &[BalanceViolation] {
        self.inner.violations()
    }

    /// Get token violation history
    pub fn token_violations(&self) -> &[TokenViolation] {
        &self.token_violations
    }

    /// Get the monitored wallet
    pub fn user_wallet(&self) -> &Pubkey {
        self.inner.user_wallet()
    }

    /// Get the configuration
    pub fn config(&self) -> &MemeBalanceGuardConfig {
        &self.config
    }

    /// Capture pre-trade SOL balance
    pub fn capture_pre_trade(&mut self, sol_balance: u64) {
        self.inner.capture_pre_trade(sol_balance);
        tracing::debug!("Meme guard: Pre-trade SOL snapshot: {} lamports", sol_balance);
    }

    /// Capture pre-trade SOL balance with slot
    pub fn capture_pre_trade_with_slot(&mut self, sol_balance: u64, slot: u64) {
        self.inner.capture_pre_trade_with_slot(sol_balance, slot);
        tracing::debug!(
            "Meme guard: Pre-trade SOL snapshot: {} lamports at slot {}",
            sol_balance,
            slot
        );
    }

    /// Capture token balance snapshot
    pub fn capture_token_snapshot(&mut self, mint: &Pubkey, balance: u64) {
        if self.config.token_tracking_enabled {
            let snapshot = TokenSnapshot::new(*mint, balance);
            self.token_snapshots.insert(*mint, snapshot);
            tracing::debug!(
                "Meme guard: Token snapshot for {}: {} tokens",
                mint,
                balance
            );
        }
    }

    /// Validate post-trade SOL balance
    pub fn validate_post_trade(
        &mut self,
        post_balance: u64,
        expected: &MemeExpectedDelta,
    ) -> Result<(), MemeBalanceGuardError> {
        // Validate SOL balance using inner guard
        self.inner
            .validate_post_trade(post_balance, &expected.base)?;

        Ok(())
    }

    /// Validate token balance change
    pub fn validate_token_delta(
        &mut self,
        mint: &Pubkey,
        post_balance: u64,
        expected_tokens: u64,
    ) -> Result<(), MemeBalanceGuardError> {
        if !self.config.token_tracking_enabled {
            return Ok(());
        }

        let pre_snapshot = self
            .token_snapshots
            .remove(mint)
            .ok_or_else(|| MemeBalanceGuardError::NoTokenSnapshot(mint.to_string()))?;

        let actual_received = post_balance.saturating_sub(pre_snapshot.balance);
        let tolerance = (expected_tokens as f64 * self.config.token_tolerance_pct) as u64;
        let min_expected = expected_tokens.saturating_sub(tolerance);

        if actual_received < min_expected {
            let violation = TokenViolation {
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                mint: *mint,
                expected: expected_tokens,
                actual: actual_received,
                tolerance_pct: self.config.token_tolerance_pct,
                reason: format!(
                    "Received {} tokens, expected {} (tolerance: {:.1}%)",
                    actual_received,
                    expected_tokens,
                    self.config.token_tolerance_pct * 100.0
                ),
            };

            tracing::warn!(
                "Token balance anomaly: mint {}, expected {}, actual {} (tolerance: {:.1}%)",
                mint,
                expected_tokens,
                actual_received,
                self.config.token_tolerance_pct * 100.0
            );

            self.token_violations.push(violation);

            return Err(MemeBalanceGuardError::TokenBalanceAnomaly {
                mint: mint.to_string(),
                expected: expected_tokens,
                actual: actual_received,
            });
        }

        tracing::debug!(
            "Token balance OK: received {} tokens (expected: {}, tolerance: {:.1}%)",
            actual_received,
            expected_tokens,
            self.config.token_tolerance_pct * 100.0
        );

        Ok(())
    }

    /// Calculate actual slippage and validate against max
    pub fn validate_slippage(
        &self,
        quote_amount: u64,
        actual_amount: u64,
        max_slippage_bps: u16,
    ) -> Result<u16, MemeBalanceGuardError> {
        if quote_amount == 0 {
            return Ok(0);
        }

        let actual_bps = if actual_amount < quote_amount {
            ((quote_amount - actual_amount) as f64 / quote_amount as f64 * 10_000.0) as u16
        } else {
            0 // Favorable slippage
        };

        if actual_bps > max_slippage_bps {
            tracing::error!(
                "Slippage exceeded: {} bps actual vs {} bps max",
                actual_bps,
                max_slippage_bps
            );
            return Err(MemeBalanceGuardError::SlippageExceeded {
                max_bps: max_slippage_bps,
                actual_bps,
            });
        }

        tracing::debug!(
            "Slippage OK: {} bps (max: {} bps)",
            actual_bps,
            max_slippage_bps
        );

        Ok(actual_bps)
    }

    /// Resume trading after review
    pub fn resume(&mut self) {
        self.inner.resume();
        tracing::warn!("Meme guard: Trading resumed after manual review");
    }

    /// Resume and persist to disk
    pub fn resume_and_persist(&mut self, data_dir: &Path) -> Result<(), MemeBalanceGuardError> {
        self.inner.resume_and_persist(data_dir)?;
        Ok(())
    }

    /// Reset cumulative tracking
    pub fn reset_cumulative(&mut self) {
        self.inner.reset_cumulative();
        self.token_violations.clear();
        self.token_snapshots.clear();
        tracing::info!("Meme guard: Cumulative tracking reset");
    }

    /// Get guard status
    pub fn create_status(&self, reason: Option<&str>) -> GuardStatus {
        self.inner.create_status(reason)
    }

    /// Save status to disk
    pub fn persist_status(
        &self,
        data_dir: &Path,
        reason: Option<&str>,
    ) -> Result<(), MemeBalanceGuardError> {
        self.inner.persist_status(data_dir, reason)?;
        Ok(())
    }

    /// Get reference to inner BalanceGuard
    pub fn inner(&self) -> &BalanceGuard {
        &self.inner
    }

    /// Get mutable reference to inner BalanceGuard
    pub fn inner_mut(&mut self) -> &mut BalanceGuard {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;
    use solana_sdk::signer::Signer;

    fn create_test_guard() -> MemeBalanceGuard {
        let wallet = Keypair::new();
        MemeBalanceGuard::new(wallet.pubkey())
    }

    fn create_test_mint() -> Pubkey {
        Keypair::new().pubkey()
    }

    #[test]
    fn test_default_config() {
        let config = MemeBalanceGuardConfig::default();
        assert_eq!(config.base_config.threshold_lamports, DEFAULT_MEME_THRESHOLD_LAMPORTS);
        assert_eq!(config.base_config.cumulative_threshold, DEFAULT_MEME_CUMULATIVE_THRESHOLD);
        assert_eq!(config.max_slippage_bps, DEFAULT_MAX_SLIPPAGE_BPS);
    }

    #[test]
    fn test_graduated_config() {
        let config = MemeBalanceGuardConfig::graduated();
        assert!(config.base_config.threshold_lamports < DEFAULT_MEME_THRESHOLD_LAMPORTS);
        assert_eq!(config.max_slippage_bps, 300);
    }

    #[test]
    fn test_pump_fun_sniper_config() {
        let config = MemeBalanceGuardConfig::pump_fun_sniper();
        assert!(config.base_config.threshold_lamports > DEFAULT_MEME_THRESHOLD_LAMPORTS);
        assert_eq!(config.max_slippage_bps, 1000);
    }

    #[test]
    fn test_sol_balance_validation_passes() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        let expected = MemeExpectedDelta::custom(-100_000_000, "test swap", 500);
        let result = guard.validate_post_trade(900_000_000, &expected); // 0.9 SOL

        assert!(result.is_ok());
        assert!(!guard.is_halted());
    }

    #[test]
    fn test_sol_balance_within_meme_threshold() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        // 8M lamports variance is within 10M threshold
        let expected = MemeExpectedDelta::custom(-100_000_000, "test", 500);
        let result = guard.validate_post_trade(892_000_000, &expected); // 0.892 SOL

        assert!(result.is_ok());
    }

    #[test]
    fn test_token_snapshot_and_validation() {
        let mut guard = create_test_guard();
        let mint = create_test_mint();

        guard.capture_token_snapshot(&mint, 0);
        let result = guard.validate_token_delta(&mint, 1000, 1000);

        assert!(result.is_ok());
    }

    #[test]
    fn test_token_within_tolerance() {
        let mut guard = create_test_guard();
        let mint = create_test_mint();

        guard.capture_token_snapshot(&mint, 0);
        // Expected 1000, got 950 (5% less, within 10% tolerance)
        let result = guard.validate_token_delta(&mint, 950, 1000);

        assert!(result.is_ok());
    }

    #[test]
    fn test_token_outside_tolerance() {
        let mut guard = create_test_guard();
        let mint = create_test_mint();

        guard.capture_token_snapshot(&mint, 0);
        // Expected 1000, got 800 (20% less, outside 10% tolerance)
        let result = guard.validate_token_delta(&mint, 800, 1000);

        assert!(matches!(
            result,
            Err(MemeBalanceGuardError::TokenBalanceAnomaly { .. })
        ));
    }

    #[test]
    fn test_slippage_validation_passes() {
        let guard = create_test_guard();
        let result = guard.validate_slippage(1000, 970, 500); // 3% slippage, 5% max

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 300); // 300 bps = 3%
    }

    #[test]
    fn test_slippage_validation_fails() {
        let guard = create_test_guard();
        let result = guard.validate_slippage(1000, 900, 50); // 10% slippage, 0.5% max

        assert!(matches!(
            result,
            Err(MemeBalanceGuardError::SlippageExceeded { .. })
        ));
    }

    #[test]
    fn test_favorable_slippage_passes() {
        let guard = create_test_guard();
        let result = guard.validate_slippage(1000, 1100, 500); // Got more than expected

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_no_token_snapshot_error() {
        let mut guard = create_test_guard();
        let mint = create_test_mint();

        let result = guard.validate_token_delta(&mint, 1000, 1000);

        assert!(matches!(
            result,
            Err(MemeBalanceGuardError::NoTokenSnapshot(_))
        ));
    }

    #[test]
    fn test_reset_clears_token_state() {
        let mut guard = create_test_guard();
        let mint = create_test_mint();

        guard.capture_token_snapshot(&mint, 0);
        guard.reset_cumulative();

        // After reset, snapshot should be gone
        let result = guard.validate_token_delta(&mint, 1000, 1000);
        assert!(matches!(
            result,
            Err(MemeBalanceGuardError::NoTokenSnapshot(_))
        ));
    }

    #[test]
    fn test_resume_clears_halt() {
        let wallet = Keypair::new();
        let config = MemeBalanceGuardConfig {
            base_config: BalanceGuardConfig {
                threshold_lamports: 1_000_000, // Low threshold to trigger
                cumulative_threshold: 10_000_000,
                halt_on_violation: true,
            },
            ..Default::default()
        };
        let mut guard = MemeBalanceGuard::with_config(wallet.pubkey(), config);

        guard.capture_pre_trade(1_000_000_000);
        let expected = MemeExpectedDelta::custom(0, "test", 500);
        let _ = guard.validate_post_trade(900_000_000, &expected); // Big unexpected loss

        assert!(guard.is_halted());

        guard.resume();
        assert!(!guard.is_halted());
    }

    #[test]
    fn test_meme_expected_delta_sol_to_meme() {
        let mint = create_test_mint();
        let delta = MemeExpectedDelta::sol_to_meme(
            100_000_000, // 0.1 SOL input
            5000,        // priority fee
            10000,       // jito tip
            500,         // 5% max slippage
            1_000_000,   // expected tokens
            mint,
        );

        assert!(delta.base.sol_change < 0);
        assert_eq!(delta.expected_tokens, Some(1_000_000));
        assert_eq!(delta.token_mint, Some(mint));
    }

    #[test]
    fn test_meme_expected_delta_meme_to_sol() {
        let delta = MemeExpectedDelta::meme_to_sol(
            100_000_000, // expected 0.1 SOL output
            5000,        // priority fee
            10000,       // jito tip
            500,         // 5% max slippage
        );

        // With 5% slippage, worst case output is 95M
        // Net = 95M - 5000 - 10000 - 5000 (gas) = ~94.98M
        assert!(delta.base.sol_change > 0);
        assert!(delta.expected_tokens.is_none());
    }
}
