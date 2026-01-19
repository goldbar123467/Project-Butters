//! Balance Guard
//!
//! Detects unexpected balance changes by comparing pre/post trade snapshots.
//! Halts trading if unexplained SOL losses exceed threshold.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Threshold in lamports for acceptable variance (0.001 SOL)
pub const DEFAULT_THRESHOLD_LAMPORTS: u64 = 1_000_000;

/// Cumulative session threshold (0.01 SOL)
pub const DEFAULT_CUMULATIVE_THRESHOLD: i64 = 10_000_000;

#[derive(Error, Debug, Clone)]
pub enum BalanceGuardError {
    #[error("Balance anomaly detected: expected {expected} lamports change, actual {actual} (diff: {diff})")]
    UnexpectedBalanceChange { expected: i64, actual: i64, diff: i64 },

    #[error("Cumulative unexplained loss {cumulative} exceeds threshold {threshold}")]
    CumulativeThresholdExceeded { cumulative: i64, threshold: i64 },

    #[error("Trading halted due to balance anomaly - manual review required")]
    TradingHalted,

    #[error("No pre-trade snapshot captured")]
    NoSnapshot,

    #[error("RPC error: {0}")]
    RpcError(String),
}

/// Point-in-time balance snapshot
#[derive(Debug, Clone)]
pub struct BalanceSnapshot {
    pub sol_balance: u64,
    pub timestamp: u64,
    pub slot: Option<u64>,
}

impl BalanceSnapshot {
    pub fn new(sol_balance: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            sol_balance,
            timestamp,
            slot: None,
        }
    }

    pub fn with_slot(mut self, slot: u64) -> Self {
        self.slot = Some(slot);
        self
    }
}

/// Expected balance delta from a trade
#[derive(Debug, Clone)]
pub struct ExpectedDelta {
    /// Expected SOL change in lamports (negative = spending)
    pub sol_change: i64,
    /// Description of what caused this delta
    pub reason: String,
}

impl ExpectedDelta {
    /// Create expected delta for SOL -> Token swap
    pub fn sol_to_token(input_lamports: u64, priority_fee: u64, jito_tip: u64) -> Self {
        let total_cost = input_lamports as i64 + priority_fee as i64 + jito_tip as i64 + 5000; // 5000 for base gas
        Self {
            sol_change: -total_cost,
            reason: format!("SOL->Token: {} input + {} fee + {} tip", input_lamports, priority_fee, jito_tip),
        }
    }

    /// Create expected delta for Token -> SOL swap
    pub fn token_to_sol(output_lamports: u64, priority_fee: u64, jito_tip: u64) -> Self {
        let net_gain = output_lamports as i64 - priority_fee as i64 - jito_tip as i64 - 5000;
        Self {
            sol_change: net_gain,
            reason: format!("Token->SOL: {} output - {} fee - {} tip", output_lamports, priority_fee, jito_tip),
        }
    }

    /// Create expected delta for a generic trade
    pub fn custom(sol_change: i64, reason: impl Into<String>) -> Self {
        Self {
            sol_change,
            reason: reason.into(),
        }
    }
}

/// Configuration for balance guard
#[derive(Debug, Clone)]
pub struct BalanceGuardConfig {
    /// Threshold for single-trade variance (lamports)
    pub threshold_lamports: u64,
    /// Threshold for cumulative session losses (lamports)
    pub cumulative_threshold: i64,
    /// Whether to halt on violation
    pub halt_on_violation: bool,
}

impl Default for BalanceGuardConfig {
    fn default() -> Self {
        Self {
            threshold_lamports: DEFAULT_THRESHOLD_LAMPORTS,
            cumulative_threshold: DEFAULT_CUMULATIVE_THRESHOLD,
            halt_on_violation: true,
        }
    }
}

/// Balance guard that tracks pre/post trade balances
#[derive(Debug)]
pub struct BalanceGuard {
    /// User wallet being monitored
    user_wallet: Pubkey,
    /// Configuration
    config: BalanceGuardConfig,
    /// Pre-trade snapshot (temporary)
    pre_trade_snapshot: Option<BalanceSnapshot>,
    /// Cumulative unexplained balance changes
    cumulative_unexplained: i64,
    /// Whether trading is halted
    is_halted: bool,
    /// History of violations
    violations: Vec<BalanceViolation>,
}

/// Record of a balance violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceViolation {
    pub timestamp: u64,
    pub expected: i64,
    pub actual: i64,
    pub diff: i64,
    pub reason: String,
}

/// Default status file name
pub const DEFAULT_STATUS_FILE: &str = "guard_status.json";

/// Persistent guard status for CLI resume functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardStatus {
    /// Whether trading is currently halted
    pub is_halted: bool,
    /// Cumulative unexplained losses (lamports)
    pub cumulative_unexplained: i64,
    /// Timestamp when status was last updated
    pub last_updated: u64,
    /// Reason for the halt (if halted)
    pub halt_reason: Option<String>,
    /// Recent violations
    pub recent_violations: Vec<BalanceViolation>,
    /// Wallet address being monitored
    pub wallet: String,
}

impl GuardStatus {
    /// Create a new halted status
    pub fn halted(wallet: &str, reason: &str, cumulative: i64, violations: Vec<BalanceViolation>) -> Self {
        Self {
            is_halted: true,
            cumulative_unexplained: cumulative,
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            halt_reason: Some(reason.to_string()),
            recent_violations: violations,
            wallet: wallet.to_string(),
        }
    }

    /// Create a resumed status
    pub fn resumed(wallet: &str) -> Self {
        Self {
            is_halted: false,
            cumulative_unexplained: 0,
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            halt_reason: None,
            recent_violations: vec![],
            wallet: wallet.to_string(),
        }
    }

    /// Load status from file
    pub fn load(data_dir: &Path) -> Result<Option<Self>, BalanceGuardError> {
        let path = data_dir.join(DEFAULT_STATUS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| BalanceGuardError::RpcError(format!("Failed to read status file: {}", e)))?;
        let status: Self = serde_json::from_str(&content)
            .map_err(|e| BalanceGuardError::RpcError(format!("Failed to parse status file: {}", e)))?;
        Ok(Some(status))
    }

    /// Save status to file
    pub fn save(&self, data_dir: &Path) -> Result<(), BalanceGuardError> {
        // Ensure directory exists
        fs::create_dir_all(data_dir)
            .map_err(|e| BalanceGuardError::RpcError(format!("Failed to create data directory: {}", e)))?;

        let path = data_dir.join(DEFAULT_STATUS_FILE);
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| BalanceGuardError::RpcError(format!("Failed to serialize status: {}", e)))?;
        fs::write(&path, content)
            .map_err(|e| BalanceGuardError::RpcError(format!("Failed to write status file: {}", e)))?;

        tracing::info!("Guard status saved to {}", path.display());
        Ok(())
    }

    /// Get the status file path
    pub fn status_file_path(data_dir: &Path) -> PathBuf {
        data_dir.join(DEFAULT_STATUS_FILE)
    }

    /// Delete the status file (used after successful resume)
    pub fn delete(data_dir: &Path) -> Result<(), BalanceGuardError> {
        let path = data_dir.join(DEFAULT_STATUS_FILE);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| BalanceGuardError::RpcError(format!("Failed to delete status file: {}", e)))?;
            tracing::info!("Guard status file deleted");
        }
        Ok(())
    }
}

impl BalanceGuard {
    /// Create a new balance guard for monitoring
    pub fn new(user_wallet: Pubkey) -> Self {
        Self::with_config(user_wallet, BalanceGuardConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(user_wallet: Pubkey, config: BalanceGuardConfig) -> Self {
        Self {
            user_wallet,
            config,
            pre_trade_snapshot: None,
            cumulative_unexplained: 0,
            is_halted: false,
            violations: Vec::new(),
        }
    }

    /// Check if trading is currently halted
    pub fn is_halted(&self) -> bool {
        self.is_halted
    }

    /// Get cumulative unexplained losses
    pub fn cumulative_unexplained(&self) -> i64 {
        self.cumulative_unexplained
    }

    /// Get violation history
    pub fn violations(&self) -> &[BalanceViolation] {
        &self.violations
    }

    /// Capture pre-trade balance snapshot
    pub fn capture_pre_trade(&mut self, sol_balance: u64) {
        self.pre_trade_snapshot = Some(BalanceSnapshot::new(sol_balance));
        tracing::debug!("Pre-trade snapshot: {} lamports", sol_balance);
    }

    /// Capture pre-trade snapshot with slot
    pub fn capture_pre_trade_with_slot(&mut self, sol_balance: u64, slot: u64) {
        self.pre_trade_snapshot = Some(BalanceSnapshot::new(sol_balance).with_slot(slot));
        tracing::debug!("Pre-trade snapshot: {} lamports at slot {}", sol_balance, slot);
    }

    /// Validate post-trade balance against expected delta
    pub fn validate_post_trade(
        &mut self,
        post_balance: u64,
        expected: &ExpectedDelta,
    ) -> Result<(), BalanceGuardError> {
        // Check if halted
        if self.is_halted {
            return Err(BalanceGuardError::TradingHalted);
        }

        // Get pre-trade snapshot
        let pre_snapshot = self.pre_trade_snapshot.take()
            .ok_or(BalanceGuardError::NoSnapshot)?;

        // Calculate actual delta
        let actual_delta = post_balance as i64 - pre_snapshot.sol_balance as i64;
        let diff = actual_delta - expected.sol_change;
        let abs_diff = diff.unsigned_abs();

        tracing::info!(
            "Balance delta check: expected {} lamports, actual {} (diff: {} lamports)",
            expected.sol_change, actual_delta, diff
        );

        // Only flag unexpected LOSSES (negative diff), not unexpected gains
        // Positive diff = user gained more than expected = favorable outcome, not a security threat
        if diff < 0 && abs_diff > self.config.threshold_lamports {
            let violation = BalanceViolation {
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                expected: expected.sol_change,
                actual: actual_delta,
                diff,
                reason: expected.reason.clone(),
            };

            self.violations.push(violation);
            self.cumulative_unexplained += diff;

            tracing::error!(
                "SECURITY: Balance anomaly! Expected {} lamports, got {} (diff: {}). Reason: {}",
                expected.sol_change, actual_delta, diff, expected.reason
            );

            // Check cumulative threshold - only halt on cumulative LOSSES
            if self.cumulative_unexplained < 0
                && self.cumulative_unexplained.abs() > self.config.cumulative_threshold {
                if self.config.halt_on_violation {
                    self.is_halted = true;
                    tracing::error!("TRADING HALTED: Cumulative unexplained loss {} exceeds threshold",
                        self.cumulative_unexplained);
                }
                return Err(BalanceGuardError::CumulativeThresholdExceeded {
                    cumulative: self.cumulative_unexplained,
                    threshold: self.config.cumulative_threshold,
                });
            }

            if self.config.halt_on_violation {
                self.is_halted = true;
            }

            return Err(BalanceGuardError::UnexpectedBalanceChange {
                expected: expected.sol_change,
                actual: actual_delta,
                diff,
            });
        } else if diff > 0 && abs_diff > self.config.threshold_lamports {
            // Log favorable variance but don't halt
            tracing::info!(
                "Favorable variance: gained {} more lamports than expected. Reason: {}",
                diff, expected.reason
            );
        }

        tracing::debug!("Balance delta OK: within {} lamport threshold", self.config.threshold_lamports);
        Ok(())
    }

    /// Manually resume trading after review
    pub fn resume(&mut self) {
        self.is_halted = false;
        tracing::warn!("Trading resumed after manual review");
    }

    /// Resume trading and persist the resumed state to disk
    pub fn resume_and_persist(&mut self, data_dir: &Path) -> Result<(), BalanceGuardError> {
        self.resume();
        let status = GuardStatus::resumed(&self.user_wallet.to_string());
        status.save(data_dir)?;
        Ok(())
    }

    /// Reset cumulative tracking (e.g., at start of new session)
    pub fn reset_cumulative(&mut self) {
        self.cumulative_unexplained = 0;
        self.violations.clear();
        tracing::info!("Balance guard cumulative tracking reset");
    }

    /// Get the monitored wallet
    pub fn user_wallet(&self) -> &Pubkey {
        &self.user_wallet
    }

    /// Create a status snapshot for persistence
    pub fn create_status(&self, reason: Option<&str>) -> GuardStatus {
        GuardStatus {
            is_halted: self.is_halted,
            cumulative_unexplained: self.cumulative_unexplained,
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            halt_reason: reason.map(String::from),
            recent_violations: self.violations.clone(),
            wallet: self.user_wallet.to_string(),
        }
    }

    /// Save current status to disk
    pub fn persist_status(&self, data_dir: &Path, reason: Option<&str>) -> Result<(), BalanceGuardError> {
        let status = self.create_status(reason);
        status.save(data_dir)
    }

    /// Check if a resume file exists and apply it
    pub fn check_and_apply_resume(data_dir: &Path) -> Result<bool, BalanceGuardError> {
        if let Some(status) = GuardStatus::load(data_dir)? {
            if !status.is_halted {
                // A resumed status file exists - the orchestrator should start unhalted
                tracing::info!("Found resume signal file - trading will start unhalted");
                // Delete the file after reading to avoid stale state
                GuardStatus::delete(data_dir)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;
    use solana_sdk::signer::Signer;

    fn create_test_guard() -> BalanceGuard {
        let wallet = Keypair::new();
        BalanceGuard::new(wallet.pubkey())
    }

    #[test]
    fn test_no_violation_when_delta_matches() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        let expected = ExpectedDelta::custom(-100_000_000, "test swap");
        let result = guard.validate_post_trade(900_000_000, &expected); // 0.9 SOL

        assert!(result.is_ok());
        assert!(!guard.is_halted());
    }

    #[test]
    fn test_violation_when_sol_lost_unexpectedly() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        // Expected to spend 0.1 SOL, but actually lost 0.2 SOL
        let expected = ExpectedDelta::custom(-100_000_000, "test swap");
        let result = guard.validate_post_trade(800_000_000, &expected); // 0.8 SOL (lost 0.1 extra)

        assert!(result.is_err());
        assert!(guard.is_halted());
    }

    #[test]
    fn test_within_threshold_passes() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000);

        // Variance of 500,000 lamports (0.0005 SOL) is within 0.001 threshold
        let expected = ExpectedDelta::custom(-100_000_000, "test");
        let result = guard.validate_post_trade(900_500_000, &expected);

        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_after_halt() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000);

        let expected = ExpectedDelta::custom(-100_000_000, "test");
        let _ = guard.validate_post_trade(700_000_000, &expected); // Big loss

        assert!(guard.is_halted());

        guard.resume();
        assert!(!guard.is_halted());
    }

    #[test]
    fn test_cumulative_threshold() {
        let config = BalanceGuardConfig {
            threshold_lamports: 20_000_000, // 0.02 SOL per-trade threshold (low to trigger violations)
            cumulative_threshold: 50_000_000, // 0.05 SOL cumulative
            halt_on_violation: false, // Don't halt on single violations so we can accumulate
        };
        let wallet = Keypair::new();
        let mut guard = BalanceGuard::with_config(wallet.pubkey(), config);

        // First trade: lose 0.03 SOL unexpectedly (exceeds 0.02 per-trade threshold)
        guard.capture_pre_trade(1_000_000_000);
        let expected = ExpectedDelta::custom(0, "test1");
        let result1 = guard.validate_post_trade(970_000_000, &expected);
        // This should fail but not halt (halt_on_violation is false)
        assert!(matches!(result1, Err(BalanceGuardError::UnexpectedBalanceChange { .. })));

        // Second trade: lose another 0.03 SOL (cumulative now 0.06 > 0.05 threshold)
        guard.capture_pre_trade(970_000_000);
        let expected = ExpectedDelta::custom(0, "test2");
        let result = guard.validate_post_trade(940_000_000, &expected);

        assert!(matches!(result, Err(BalanceGuardError::CumulativeThresholdExceeded { .. })));
    }

    #[test]
    fn test_sol_to_token_delta() {
        let delta = ExpectedDelta::sol_to_token(100_000_000, 5000, 10000);
        assert!(delta.sol_change < 0);
        assert!(delta.reason.contains("SOL->Token"));
    }

    #[test]
    fn test_token_to_sol_delta() {
        let delta = ExpectedDelta::token_to_sol(100_000_000, 5000, 10000);
        assert!(delta.sol_change > 0);
        assert!(delta.reason.contains("Token->SOL"));
    }

    #[test]
    fn test_no_snapshot_error() {
        let mut guard = create_test_guard();
        let expected = ExpectedDelta::custom(0, "test");
        let result = guard.validate_post_trade(1_000_000_000, &expected);

        assert!(matches!(result, Err(BalanceGuardError::NoSnapshot)));
    }

    #[test]
    fn test_halted_returns_error() {
        let mut guard = create_test_guard();
        guard.is_halted = true;
        guard.capture_pre_trade(1_000_000_000);

        let expected = ExpectedDelta::custom(0, "test");
        let result = guard.validate_post_trade(1_000_000_000, &expected);

        assert!(matches!(result, Err(BalanceGuardError::TradingHalted)));
    }

    #[test]
    fn test_violation_history() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000);

        let expected = ExpectedDelta::custom(0, "suspicious trade");
        let _ = guard.validate_post_trade(800_000_000, &expected);

        assert_eq!(guard.violations().len(), 1);
        assert!(guard.violations()[0].reason.contains("suspicious"));
    }

    #[test]
    fn test_unexpected_gain_does_not_halt() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        // Expected to gain 0.1 SOL, actually gained 0.2 SOL (favorable variance)
        let expected = ExpectedDelta::custom(100_000_000, "token->SOL swap");
        let result = guard.validate_post_trade(1_200_000_000, &expected); // 1.2 SOL

        // Should NOT halt on gains - this is favorable for the user
        assert!(result.is_ok());
        assert!(!guard.is_halted());
        assert_eq!(guard.violations().len(), 0); // No violations for gains
    }

    #[test]
    fn test_large_unexpected_gain_logs_but_passes() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        // Expected small gain, got massive gain (very favorable slippage)
        let expected = ExpectedDelta::custom(10_000_000, "token->SOL"); // expect +0.01 SOL
        let result = guard.validate_post_trade(2_000_000_000, &expected); // got +1 SOL!

        // Even with huge favorable variance, should not halt
        assert!(result.is_ok());
        assert!(!guard.is_halted());
    }

    #[test]
    fn test_loss_still_triggers_halt() {
        let mut guard = create_test_guard();
        guard.capture_pre_trade(1_000_000_000); // 1 SOL

        // Expected to spend 0.1 SOL, actually lost 0.3 SOL (unexpected loss)
        let expected = ExpectedDelta::custom(-100_000_000, "SOL->token swap");
        let result = guard.validate_post_trade(700_000_000, &expected); // only 0.7 SOL left

        // SHOULD halt on unexpected losses
        assert!(result.is_err());
        assert!(guard.is_halted());
    }
}
