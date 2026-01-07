//! Jito Configuration
//!
//! Configuration for Jito Block Engine connection and bundle submission.

use std::time::Duration;

/// Jito Block Engine endpoints
pub mod endpoints {
    /// Mainnet block engine (Amsterdam)
    pub const MAINNET_AMSTERDAM: &str = "https://amsterdam.mainnet.block-engine.jito.wtf";
    /// Mainnet block engine (Frankfurt)
    pub const MAINNET_FRANKFURT: &str = "https://frankfurt.mainnet.block-engine.jito.wtf";
    /// Mainnet block engine (New York)
    pub const MAINNET_NY: &str = "https://ny.mainnet.block-engine.jito.wtf";
    /// Mainnet block engine (Tokyo)
    pub const MAINNET_TOKYO: &str = "https://tokyo.mainnet.block-engine.jito.wtf";
    /// Default mainnet endpoint
    pub const MAINNET_DEFAULT: &str = MAINNET_NY;
}

/// Jito tip accounts for validator tips
pub mod tip_accounts {
    /// Official Jito tip accounts (validators rotate through these)
    pub const TIP_ACCOUNTS: &[&str] = &[
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
        "HFqU5x63VTqvQss8hp11i4bVmkdzGZBJLYQ6QwBvp8dx",
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    ];

    /// Get a random tip account
    pub fn random_tip_account() -> &'static str {
        use rand::Rng;
        let idx = rand::thread_rng().gen_range(0..TIP_ACCOUNTS.len());
        TIP_ACCOUNTS[idx]
    }
}

/// Jito Block Engine configuration
#[derive(Debug, Clone)]
pub struct JitoConfig {
    /// Block Engine endpoint URL
    pub block_engine_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Number of retry attempts
    pub max_retries: u32,
    /// Retry delay base (exponential backoff)
    pub retry_delay_ms: u64,
    /// Default tip amount in lamports
    pub default_tip_lamports: u64,
    /// Optional API token for authenticated requests
    pub api_token: Option<String>,
}

impl Default for JitoConfig {
    fn default() -> Self {
        Self {
            block_engine_url: endpoints::MAINNET_DEFAULT.to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_delay_ms: 500,
            default_tip_lamports: 10_000, // 0.00001 SOL
            api_token: None,
        }
    }
}

impl JitoConfig {
    /// Create config for mainnet with specific region
    pub fn mainnet(region: &str) -> Self {
        let url = match region.to_lowercase().as_str() {
            "amsterdam" | "ams" => endpoints::MAINNET_AMSTERDAM,
            "frankfurt" | "fra" => endpoints::MAINNET_FRANKFURT,
            "tokyo" | "tyo" => endpoints::MAINNET_TOKYO,
            "newyork" | "ny" | _ => endpoints::MAINNET_NY,
        };

        Self {
            block_engine_url: url.to_string(),
            ..Default::default()
        }
    }

    /// Set tip amount in lamports
    pub fn with_tip(mut self, lamports: u64) -> Self {
        self.default_tip_lamports = lamports;
        self
    }

    /// Set API token
    pub fn with_api_token(mut self, token: String) -> Self {
        self.api_token = Some(token);
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = JitoConfig::default();
        assert_eq!(config.block_engine_url, endpoints::MAINNET_NY);
        assert_eq!(config.default_tip_lamports, 10_000);
        assert!(config.api_token.is_none());
    }

    #[test]
    fn test_mainnet_regions() {
        let ams = JitoConfig::mainnet("amsterdam");
        assert!(ams.block_engine_url.contains("amsterdam"));

        let fra = JitoConfig::mainnet("fra");
        assert!(fra.block_engine_url.contains("frankfurt"));

        let tyo = JitoConfig::mainnet("tokyo");
        assert!(tyo.block_engine_url.contains("tokyo"));
    }

    #[test]
    fn test_builder_methods() {
        let config = JitoConfig::default()
            .with_tip(50_000)
            .with_api_token("test-token".to_string())
            .with_timeout(Duration::from_secs(60));

        assert_eq!(config.default_tip_lamports, 50_000);
        assert_eq!(config.api_token, Some("test-token".to_string()));
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_tip_accounts_not_empty() {
        assert!(!tip_accounts::TIP_ACCOUNTS.is_empty());
    }

    #[test]
    fn test_random_tip_account() {
        let tip = tip_accounts::random_tip_account();
        assert!(tip_accounts::TIP_ACCOUNTS.contains(&tip));
    }
}
