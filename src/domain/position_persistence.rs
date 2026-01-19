//! Position Persistence
//!
//! Crash recovery module that persists position state to disk,
//! enabling recovery after unexpected shutdowns.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Default position file name
pub const DEFAULT_POSITION_FILE: &str = "active_position.json";

#[derive(Error, Debug, Clone)]
pub enum PersistError {
    #[error("Failed to serialize position: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize position: {0}")]
    DeserializationError(String),

    #[error("Failed to write position file: {0}")]
    WriteError(String),

    #[error("Failed to read position file: {0}")]
    ReadError(String),

    #[error("Failed to delete position file: {0}")]
    DeleteError(String),

    #[error("Position file is corrupted: {0}")]
    CorruptedFile(String),

    #[error("Failed to create directory: {0}")]
    DirectoryError(String),
}

/// Persisted position data for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPosition {
    /// Token mint address
    pub token_mint: String,
    /// Token symbol for display
    pub token_symbol: String,
    /// Entry price in USD
    pub entry_price: f64,
    /// Entry timestamp (Unix seconds)
    pub entry_time: u64,
    /// Token amount in base units
    pub amount: u64,
    /// Transaction signature of the entry trade
    pub entry_tx_signature: String,
    /// USDC spent to enter the position
    pub usdc_spent: f64,
    /// Stop loss price (optional)
    pub stop_loss_price: Option<f64>,
    /// Take profit price (optional)
    pub take_profit_price: Option<f64>,
    /// Token decimals
    pub token_decimals: u8,
    /// Position metadata (optional notes)
    pub metadata: Option<PositionMetadata>,
}

/// Additional metadata for persisted positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionMetadata {
    /// Strategy that opened this position
    pub strategy: String,
    /// Signal confidence when entered (0-100)
    pub signal_confidence: f64,
    /// Z-score at entry (for mean reversion)
    pub entry_zscore: Option<f64>,
    /// Notes about the position
    pub notes: Option<String>,
}

/// Recovery status after loading a position
#[derive(Debug, Clone)]
pub enum RecoveryStatus {
    /// No position to recover
    NoPosition,
    /// Position recovered successfully
    Recovered(PersistedPosition),
    /// Position file corrupted, manual intervention needed
    Corrupted(String),
}

impl PersistedPosition {
    /// Create a new persisted position
    pub fn new(
        token_mint: String,
        token_symbol: String,
        entry_price: f64,
        entry_time: u64,
        amount: u64,
        entry_tx_signature: String,
        usdc_spent: f64,
    ) -> Self {
        Self {
            token_mint,
            token_symbol,
            entry_price,
            entry_time,
            amount,
            entry_tx_signature,
            usdc_spent,
            stop_loss_price: None,
            take_profit_price: None,
            token_decimals: 9, // Default to 9 (SOL-like)
            metadata: None,
        }
    }

    /// Create with full configuration
    pub fn with_config(
        token_mint: String,
        token_symbol: String,
        entry_price: f64,
        entry_time: u64,
        amount: u64,
        entry_tx_signature: String,
        usdc_spent: f64,
        stop_loss_price: Option<f64>,
        take_profit_price: Option<f64>,
        token_decimals: u8,
    ) -> Self {
        Self {
            token_mint,
            token_symbol,
            entry_price,
            entry_time,
            amount,
            entry_tx_signature,
            usdc_spent,
            stop_loss_price,
            take_profit_price,
            token_decimals,
            metadata: None,
        }
    }

    /// Set metadata for the position
    pub fn with_metadata(mut self, metadata: PositionMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Calculate current PnL given current price
    pub fn calculate_pnl(&self, current_price: f64) -> f64 {
        let current_value = self.token_value_at_price(current_price);
        current_value - self.usdc_spent
    }

    /// Calculate current PnL percentage
    pub fn calculate_pnl_pct(&self, current_price: f64) -> f64 {
        if self.usdc_spent == 0.0 {
            return 0.0;
        }
        (self.calculate_pnl(current_price) / self.usdc_spent) * 100.0
    }

    /// Calculate token value at given price
    pub fn token_value_at_price(&self, price: f64) -> f64 {
        let amount_adjusted = self.amount as f64 / 10_f64.powi(self.token_decimals as i32);
        amount_adjusted * price
    }

    /// Check if stop loss is triggered
    pub fn is_stop_loss_triggered(&self, current_price: f64) -> bool {
        self.stop_loss_price.map_or(false, |sl| current_price <= sl)
    }

    /// Check if take profit is triggered
    pub fn is_take_profit_triggered(&self, current_price: f64) -> bool {
        self.take_profit_price.map_or(false, |tp| current_price >= tp)
    }

    /// Get the position age in seconds
    pub fn age_seconds(&self, current_time: u64) -> u64 {
        current_time.saturating_sub(self.entry_time)
    }

    /// Get the position age in hours
    pub fn age_hours(&self, current_time: u64) -> f64 {
        self.age_seconds(current_time) as f64 / 3600.0
    }

    /// Save position to disk
    pub fn save(&self, path: &Path) -> Result<(), PersistError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| PersistError::DirectoryError(e.to_string()))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| PersistError::SerializationError(e.to_string()))?;

        fs::write(path, content)
            .map_err(|e| PersistError::WriteError(e.to_string()))?;

        tracing::info!(
            "Position saved: {} {} @ ${:.4} (tx: {})",
            self.token_symbol,
            self.amount,
            self.entry_price,
            &self.entry_tx_signature[..8]
        );

        Ok(())
    }

    /// Load position from disk
    pub fn load(path: &Path) -> Result<Option<Self>, PersistError> {
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)
            .map_err(|e| PersistError::ReadError(e.to_string()))?;

        if content.trim().is_empty() {
            return Ok(None);
        }

        let position: Self = serde_json::from_str(&content)
            .map_err(|e| PersistError::DeserializationError(e.to_string()))?;

        tracing::info!(
            "Position loaded: {} {} @ ${:.4}",
            position.token_symbol,
            position.amount,
            position.entry_price
        );

        Ok(Some(position))
    }

    /// Delete position file
    pub fn delete(path: &Path) -> Result<(), PersistError> {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|e| PersistError::DeleteError(e.to_string()))?;
            tracing::info!("Position file deleted: {}", path.display());
        }
        Ok(())
    }

    /// Check if position file exists
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    /// Try to recover position with validation
    pub fn try_recover(path: &Path) -> RecoveryStatus {
        if !path.exists() {
            return RecoveryStatus::NoPosition;
        }

        match Self::load(path) {
            Ok(Some(position)) => {
                // Validate the position data
                if position.token_mint.is_empty() {
                    return RecoveryStatus::Corrupted("Empty token mint".to_string());
                }
                if position.amount == 0 {
                    return RecoveryStatus::Corrupted("Zero amount".to_string());
                }
                if position.entry_price <= 0.0 {
                    return RecoveryStatus::Corrupted("Invalid entry price".to_string());
                }
                RecoveryStatus::Recovered(position)
            }
            Ok(None) => RecoveryStatus::NoPosition,
            Err(e) => RecoveryStatus::Corrupted(e.to_string()),
        }
    }

    /// Get default position file path for a data directory
    pub fn default_path(data_dir: &Path) -> std::path::PathBuf {
        data_dir.join(DEFAULT_POSITION_FILE)
    }
}

/// Position persistence manager for multiple positions
#[derive(Debug)]
pub struct PositionManager {
    data_dir: std::path::PathBuf,
}

impl PositionManager {
    /// Create a new position manager
    pub fn new(data_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// Get the active position file path
    pub fn position_path(&self) -> std::path::PathBuf {
        PersistedPosition::default_path(&self.data_dir)
    }

    /// Save the active position
    pub fn save_position(&self, position: &PersistedPosition) -> Result<(), PersistError> {
        position.save(&self.position_path())
    }

    /// Load the active position
    pub fn load_position(&self) -> Result<Option<PersistedPosition>, PersistError> {
        PersistedPosition::load(&self.position_path())
    }

    /// Delete the active position
    pub fn delete_position(&self) -> Result<(), PersistError> {
        PersistedPosition::delete(&self.position_path())
    }

    /// Check if there's an active position
    pub fn has_position(&self) -> bool {
        PersistedPosition::exists(&self.position_path())
    }

    /// Try to recover on startup
    pub fn try_recover(&self) -> RecoveryStatus {
        PersistedPosition::try_recover(&self.position_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_test_position() -> PersistedPosition {
        PersistedPosition::new(
            "TokenMint111111111111111111111111111111111".to_string(),
            "TEST".to_string(),
            1.5,
            1700000000,
            1_000_000_000, // 1 token with 9 decimals
            "TxSignature11111111111111111111111111111111111111111111111111111111".to_string(),
            1.5,
        )
    }

    #[test]
    fn test_new_position() {
        let pos = create_test_position();
        assert_eq!(pos.token_symbol, "TEST");
        assert_eq!(pos.entry_price, 1.5);
        assert_eq!(pos.amount, 1_000_000_000);
    }

    #[test]
    fn test_with_config() {
        let pos = PersistedPosition::with_config(
            "Mint".to_string(),
            "TEST".to_string(),
            1.5,
            1000,
            1_000_000,
            "tx".to_string(),
            1.5,
            Some(1.0),
            Some(2.0),
            6,
        );

        assert_eq!(pos.stop_loss_price, Some(1.0));
        assert_eq!(pos.take_profit_price, Some(2.0));
        assert_eq!(pos.token_decimals, 6);
    }

    #[test]
    fn test_calculate_pnl() {
        let pos = create_test_position();

        // Breakeven
        assert!((pos.calculate_pnl(1.5) - 0.0).abs() < 0.001);

        // 10% profit
        let pnl = pos.calculate_pnl(1.65);
        assert!((pnl - 0.15).abs() < 0.001);

        // 10% loss
        let pnl = pos.calculate_pnl(1.35);
        assert!((pnl - (-0.15)).abs() < 0.001);
    }

    #[test]
    fn test_calculate_pnl_pct() {
        let pos = create_test_position();

        // 10% profit
        let pnl_pct = pos.calculate_pnl_pct(1.65);
        assert!((pnl_pct - 10.0).abs() < 0.1);

        // 10% loss
        let pnl_pct = pos.calculate_pnl_pct(1.35);
        assert!((pnl_pct - (-10.0)).abs() < 0.1);
    }

    #[test]
    fn test_stop_loss_triggered() {
        let mut pos = create_test_position();
        pos.stop_loss_price = Some(1.2);

        assert!(!pos.is_stop_loss_triggered(1.5));
        assert!(!pos.is_stop_loss_triggered(1.3));
        assert!(pos.is_stop_loss_triggered(1.2));
        assert!(pos.is_stop_loss_triggered(1.0));
    }

    #[test]
    fn test_take_profit_triggered() {
        let mut pos = create_test_position();
        pos.take_profit_price = Some(2.0);

        assert!(!pos.is_take_profit_triggered(1.5));
        assert!(!pos.is_take_profit_triggered(1.9));
        assert!(pos.is_take_profit_triggered(2.0));
        assert!(pos.is_take_profit_triggered(2.5));
    }

    #[test]
    fn test_age_calculation() {
        let pos = create_test_position();
        let current_time = pos.entry_time + 7200; // 2 hours later

        assert_eq!(pos.age_seconds(current_time), 7200);
        assert!((pos.age_hours(current_time) - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        let pos = create_test_position();
        pos.save(&path).unwrap();

        let loaded = PersistedPosition::load(&path).unwrap().unwrap();
        assert_eq!(loaded.token_symbol, pos.token_symbol);
        assert_eq!(loaded.entry_price, pos.entry_price);
        assert_eq!(loaded.amount, pos.amount);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let result = PersistedPosition::load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        let pos = create_test_position();
        pos.save(&path).unwrap();
        assert!(path.exists());

        PersistedPosition::delete(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_exists() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        assert!(!PersistedPosition::exists(&path));

        let pos = create_test_position();
        pos.save(&path).unwrap();

        assert!(PersistedPosition::exists(&path));
    }

    #[test]
    fn test_try_recover_no_position() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        let status = PersistedPosition::try_recover(&path);
        assert!(matches!(status, RecoveryStatus::NoPosition));
    }

    #[test]
    fn test_try_recover_valid_position() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        let pos = create_test_position();
        pos.save(&path).unwrap();

        let status = PersistedPosition::try_recover(&path);
        assert!(matches!(status, RecoveryStatus::Recovered(_)));
    }

    #[test]
    fn test_try_recover_corrupted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        // Write invalid JSON
        fs::write(&path, "{ invalid json }").unwrap();

        let status = PersistedPosition::try_recover(&path);
        assert!(matches!(status, RecoveryStatus::Corrupted(_)));
    }

    #[test]
    fn test_try_recover_invalid_data() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("position.json");

        // Write valid JSON but invalid position
        let invalid_pos = PersistedPosition::new(
            "".to_string(), // Empty mint
            "TEST".to_string(),
            1.5,
            1000,
            1000,
            "tx".to_string(),
            1.5,
        );
        invalid_pos.save(&path).unwrap();

        let status = PersistedPosition::try_recover(&path);
        assert!(matches!(status, RecoveryStatus::Corrupted(_)));
    }

    #[test]
    fn test_position_manager() {
        let dir = tempdir().unwrap();
        let manager = PositionManager::new(dir.path());

        assert!(!manager.has_position());

        let pos = create_test_position();
        manager.save_position(&pos).unwrap();

        assert!(manager.has_position());

        let loaded = manager.load_position().unwrap().unwrap();
        assert_eq!(loaded.token_symbol, pos.token_symbol);

        manager.delete_position().unwrap();
        assert!(!manager.has_position());
    }

    #[test]
    fn test_with_metadata() {
        let pos = create_test_position().with_metadata(PositionMetadata {
            strategy: "mean_reversion".to_string(),
            signal_confidence: 85.0,
            entry_zscore: Some(-2.5),
            notes: Some("Test position".to_string()),
        });

        assert!(pos.metadata.is_some());
        let meta = pos.metadata.unwrap();
        assert_eq!(meta.strategy, "mean_reversion");
        assert_eq!(meta.signal_confidence, 85.0);
    }

    #[test]
    fn test_token_value_at_price() {
        let pos = create_test_position();

        // 1 token at $1.5 = $1.5
        let value = pos.token_value_at_price(1.5);
        assert!((value - 1.5).abs() < 0.001);

        // 1 token at $2.0 = $2.0
        let value = pos.token_value_at_price(2.0);
        assert!((value - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_default_path() {
        let data_dir = Path::new("/tmp/data");
        let path = PersistedPosition::default_path(data_dir);
        assert_eq!(path, Path::new("/tmp/data/active_position.json"));
    }

    #[test]
    fn test_save_creates_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir").join("nested").join("position.json");

        let pos = create_test_position();
        pos.save(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_delete_nonexistent_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        // Should not error when file doesn't exist
        let result = PersistedPosition::delete(&path);
        assert!(result.is_ok());
    }
}
