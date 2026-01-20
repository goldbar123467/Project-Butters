//! Meme Coin Trading Traits
//!
//! Trait definitions for meme coin trading functionality including
//! launch detection and token analysis capabilities.

use async_trait::async_trait;
use thiserror::Error;

use super::types::{MemeEntrySignal, OUParams, TokenInfo};

/// Errors that can occur during launch detection
#[derive(Debug, Error)]
pub enum LaunchDetectorError {
    /// Network or API error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Token metadata unavailable
    #[error("Token metadata unavailable: {0}")]
    MetadataUnavailable(String),

    /// Rate limited by API
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Errors that can occur during token analysis
#[derive(Debug, Error)]
pub enum TokenAnalyzerError {
    /// Insufficient price data
    #[error("Insufficient price data: need {required} samples, have {available}")]
    InsufficientData { required: usize, available: usize },

    /// OU parameter estimation failed
    #[error("OU estimation failed: {0}")]
    OUEstimationFailed(String),

    /// Price feed unavailable
    #[error("Price feed unavailable: {0}")]
    PriceFeedUnavailable(String),

    /// Token not supported
    #[error("Token not supported: {0}")]
    TokenNotSupported(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Launch detection results
#[derive(Debug, Clone)]
pub struct LaunchInfo {
    /// Token mint address
    pub mint: String,
    /// Token symbol (if available)
    pub symbol: Option<String>,
    /// Token decimals
    pub decimals: u8,
    /// Initial liquidity in USDC
    pub liquidity_usdc: f64,
    /// Launch timestamp (Unix seconds)
    pub launch_timestamp: u64,
    /// Creator/deployer address
    pub creator: Option<String>,
    /// Launch platform (e.g., "pump.fun", "raydium")
    pub platform: Option<String>,
}

/// Token analysis results
#[derive(Debug, Clone)]
pub struct TokenAnalysis {
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Current price in USDC
    pub current_price: f64,
    /// OU process parameters (if estimatable)
    pub ou_params: Option<OUParams>,
    /// Current z-score (if OU params available)
    pub z_score: Option<f64>,
    /// Half-life in minutes (if OU params available)
    pub half_life_minutes: Option<f64>,
    /// Whether token meets tradeability criteria
    pub is_tradeable: bool,
    /// Reason for non-tradeability (if applicable)
    pub non_tradeable_reason: Option<String>,
    /// 24h volume in USDC
    pub volume_24h_usdc: Option<f64>,
    /// Number of holders
    pub holder_count: Option<u64>,
}

/// Trait for detecting new token launches on Solana
///
/// Implementations of this trait monitor various launch platforms
/// (pump.fun, Raydium, etc.) for new token deployments.
#[async_trait]
pub trait LaunchDetector: Send + Sync {
    /// Get the name of this detector
    fn name(&self) -> &str;

    /// Check for new token launches since the given timestamp
    ///
    /// # Arguments
    /// * `since_timestamp` - Unix timestamp to check launches from
    /// * `limit` - Maximum number of launches to return
    ///
    /// # Returns
    /// A vector of newly detected launches
    async fn detect_launches(
        &self,
        since_timestamp: u64,
        limit: usize,
    ) -> Result<Vec<LaunchInfo>, LaunchDetectorError>;

    /// Check if a specific token is a recently launched token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `max_age_hours` - Maximum age in hours to consider "recent"
    ///
    /// # Returns
    /// Launch info if token is recent, None otherwise
    async fn is_recent_launch(
        &self,
        mint: &str,
        max_age_hours: f64,
    ) -> Result<Option<LaunchInfo>, LaunchDetectorError>;

    /// Get supported launch platforms
    fn supported_platforms(&self) -> Vec<String>;
}

/// Trait for analyzing meme tokens and generating trading signals
///
/// Implementations of this trait track token prices, estimate OU parameters,
/// and determine entry/exit signals for the meme trading strategy.
#[async_trait]
pub trait TokenAnalyzer: Send + Sync {
    /// Get the name of this analyzer
    fn name(&self) -> &str;

    /// Start tracking a token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `symbol` - Token symbol
    /// * `decimals` - Token decimals
    ///
    /// # Returns
    /// Ok(()) if tracking started successfully
    async fn start_tracking(
        &mut self,
        mint: &str,
        symbol: &str,
        decimals: u8,
    ) -> Result<(), TokenAnalyzerError>;

    /// Stop tracking a token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    async fn stop_tracking(&mut self, mint: &str);

    /// Update price data for all tracked tokens
    ///
    /// This should be called periodically (e.g., every poll interval)
    /// to fetch latest prices and update OU parameter estimates.
    async fn update(&mut self) -> Result<(), TokenAnalyzerError>;

    /// Get current analysis for a specific token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    ///
    /// # Returns
    /// Token analysis if tracking, None otherwise
    fn get_analysis(&self, mint: &str) -> Option<TokenAnalysis>;

    /// Get analysis for all tracked tokens
    fn get_all_analyses(&self) -> Vec<TokenAnalysis>;

    /// Get token info for a tracked token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    ///
    /// # Returns
    /// Token info if tracking, None otherwise
    fn get_token_info(&self, mint: &str) -> Option<TokenInfo>;

    /// Check for entry signals across all tracked tokens
    ///
    /// # Arguments
    /// * `z_threshold` - Z-score threshold for entry (negative value)
    /// * `min_confidence` - Minimum OU parameter confidence
    ///
    /// # Returns
    /// Entry signals for tokens meeting criteria
    fn check_entry_signals(
        &self,
        z_threshold: f64,
        min_confidence: f64,
    ) -> Vec<MemeEntrySignal>;

    /// Check if a token should exit based on current conditions
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `entry_price` - Entry price
    /// * `z_exit_threshold` - Z-score threshold for exit
    /// * `stop_loss_pct` - Stop loss percentage
    /// * `take_profit_pct` - Take profit percentage
    ///
    /// # Returns
    /// True if exit signal triggered
    fn should_exit(
        &self,
        mint: &str,
        entry_price: f64,
        z_exit_threshold: f64,
        stop_loss_pct: f64,
        take_profit_pct: f64,
    ) -> bool;

    /// Get the number of tracked tokens
    fn tracked_count(&self) -> usize;

    /// Check if analyzer has enough data to generate signals
    fn is_ready(&self) -> bool;

    /// Reset all tracking data
    fn reset(&mut self);
}

/// Stub implementation of LaunchDetector for testing
pub struct StubLaunchDetector {
    name: String,
}

impl StubLaunchDetector {
    /// Create a new stub launch detector
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl LaunchDetector for StubLaunchDetector {
    fn name(&self) -> &str {
        &self.name
    }

    async fn detect_launches(
        &self,
        _since_timestamp: u64,
        _limit: usize,
    ) -> Result<Vec<LaunchInfo>, LaunchDetectorError> {
        // Stub: return empty list
        Ok(vec![])
    }

    async fn is_recent_launch(
        &self,
        _mint: &str,
        _max_age_hours: f64,
    ) -> Result<Option<LaunchInfo>, LaunchDetectorError> {
        // Stub: return None
        Ok(None)
    }

    fn supported_platforms(&self) -> Vec<String> {
        vec!["stub".to_string()]
    }
}

/// Stub implementation of TokenAnalyzer for testing
pub struct StubTokenAnalyzer {
    name: String,
    tracked_tokens: Vec<String>,
}

impl StubTokenAnalyzer {
    /// Create a new stub token analyzer
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tracked_tokens: vec![],
        }
    }
}

#[async_trait]
impl TokenAnalyzer for StubTokenAnalyzer {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start_tracking(
        &mut self,
        mint: &str,
        _symbol: &str,
        _decimals: u8,
    ) -> Result<(), TokenAnalyzerError> {
        if !self.tracked_tokens.contains(&mint.to_string()) {
            self.tracked_tokens.push(mint.to_string());
        }
        Ok(())
    }

    async fn stop_tracking(&mut self, mint: &str) {
        self.tracked_tokens.retain(|m| m != mint);
    }

    async fn update(&mut self) -> Result<(), TokenAnalyzerError> {
        // Stub: no-op
        Ok(())
    }

    fn get_analysis(&self, _mint: &str) -> Option<TokenAnalysis> {
        // Stub: return None
        None
    }

    fn get_all_analyses(&self) -> Vec<TokenAnalysis> {
        // Stub: return empty
        vec![]
    }

    fn get_token_info(&self, _mint: &str) -> Option<TokenInfo> {
        // Stub: return None
        None
    }

    fn check_entry_signals(
        &self,
        _z_threshold: f64,
        _min_confidence: f64,
    ) -> Vec<MemeEntrySignal> {
        // Stub: return empty
        vec![]
    }

    fn should_exit(
        &self,
        _mint: &str,
        _entry_price: f64,
        _z_exit_threshold: f64,
        _stop_loss_pct: f64,
        _take_profit_pct: f64,
    ) -> bool {
        // Stub: always false
        false
    }

    fn tracked_count(&self) -> usize {
        self.tracked_tokens.len()
    }

    fn is_ready(&self) -> bool {
        // Stub: always false
        false
    }

    fn reset(&mut self) {
        self.tracked_tokens.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stub_launch_detector() {
        let detector = StubLaunchDetector::new("test");
        assert_eq!(detector.name(), "test");
        assert_eq!(detector.supported_platforms(), vec!["stub".to_string()]);

        let launches = detector.detect_launches(0, 10).await.unwrap();
        assert!(launches.is_empty());

        let recent = detector.is_recent_launch("test_mint", 24.0).await.unwrap();
        assert!(recent.is_none());
    }

    #[tokio::test]
    async fn test_stub_token_analyzer() {
        let mut analyzer = StubTokenAnalyzer::new("test");
        assert_eq!(analyzer.name(), "test");
        assert_eq!(analyzer.tracked_count(), 0);
        assert!(!analyzer.is_ready());

        // Start tracking
        analyzer.start_tracking("mint1", "TOKEN1", 9).await.unwrap();
        assert_eq!(analyzer.tracked_count(), 1);

        // Stop tracking
        analyzer.stop_tracking("mint1").await;
        assert_eq!(analyzer.tracked_count(), 0);
    }

    #[tokio::test]
    async fn test_stub_analyzer_signals() {
        let analyzer = StubTokenAnalyzer::new("test");

        let signals = analyzer.check_entry_signals(-3.5, 0.3);
        assert!(signals.is_empty());

        assert!(!analyzer.should_exit("mint", 100.0, 0.0, 10.0, 15.0));
    }
}
