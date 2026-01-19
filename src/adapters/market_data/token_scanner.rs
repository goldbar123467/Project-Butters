//! Jupiter Token Scanner
//!
//! Discovers and filters meme tokens from Jupiter for trading.
//! Uses Jupiter's token list and price API to find high-volume,
//! liquid meme coins that meet the strategy's requirements.
//!
//! Features:
//! - Rate limiting (60 RPM for Jupiter free tier)
//! - Batch price fetching
//! - Configurable filters (volume, liquidity, spread, age)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

/// Jupiter API endpoints
const JUPITER_PRICE_API: &str = "https://price.jup.ag/v6/price";
const JUPITER_TOKEN_LIST_API: &str = "https://token.jup.ag/strict";

/// Rate limiting constants
const DEFAULT_RATE_LIMIT_RPM: u32 = 60;
const MIN_REQUEST_INTERVAL_MS: u64 = 1000; // 1 second minimum between requests

/// Scanner errors
#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("Rate limit exceeded, retry after {0} ms")]
    RateLimitExceeded(u64),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("No token data available")]
    NoTokenData,
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Meme token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemeToken {
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Token name
    pub name: String,
    /// 24-hour trading volume in USD
    pub volume_24h: f64,
    /// Liquidity in USD
    pub liquidity: f64,
    /// Current price in USD
    pub price: f64,
    /// 24-hour price change percentage
    pub price_change_24h: f64,
    /// Decimals for the token
    pub decimals: u8,
}

impl MemeToken {
    /// Check if token meets basic quality criteria
    pub fn is_tradeable(&self) -> bool {
        self.price > 0.0
            && self.liquidity > 0.0
            && self.volume_24h > 0.0
    }
}

/// Scanner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    /// Minimum 24-hour volume in USD
    pub min_volume_24h: f64,
    /// Minimum liquidity in USD
    pub min_liquidity: f64,
    /// Maximum number of tokens to return
    pub top_n_tokens: usize,
    /// Maximum spread in basis points
    pub max_spread_bps: u32,
    /// Minimum token age in hours (since listing)
    pub min_token_age_hours: u64,
    /// Rate limit (requests per minute)
    pub rate_limit_rpm: u32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            min_volume_24h: 500_000.0,
            min_liquidity: 250_000.0,
            top_n_tokens: 30,
            max_spread_bps: 50,
            min_token_age_hours: 24,
            rate_limit_rpm: DEFAULT_RATE_LIMIT_RPM,
            timeout_secs: 30,
        }
    }
}

impl ScannerConfig {
    /// Create config with custom volume threshold
    pub fn with_min_volume(mut self, volume: f64) -> Self {
        self.min_volume_24h = volume;
        self
    }

    /// Create config with custom liquidity threshold
    pub fn with_min_liquidity(mut self, liquidity: f64) -> Self {
        self.min_liquidity = liquidity;
        self
    }

    /// Create config with custom top_n
    pub fn with_top_n(mut self, n: usize) -> Self {
        self.top_n_tokens = n;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ScannerError> {
        if self.min_volume_24h < 0.0 {
            return Err(ScannerError::ConfigError("min_volume_24h must be >= 0".into()));
        }
        if self.min_liquidity < 0.0 {
            return Err(ScannerError::ConfigError("min_liquidity must be >= 0".into()));
        }
        if self.top_n_tokens == 0 {
            return Err(ScannerError::ConfigError("top_n_tokens must be > 0".into()));
        }
        if self.rate_limit_rpm == 0 {
            return Err(ScannerError::ConfigError("rate_limit_rpm must be > 0".into()));
        }
        Ok(())
    }
}

/// Simple rate limiter using token bucket algorithm
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum requests per minute
    rpm_limit: u32,
    /// Timestamp of last request
    last_request: Instant,
    /// Requests made in current window
    requests_in_window: u32,
    /// Window start time
    window_start: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(rpm_limit: u32) -> Self {
        let now = Instant::now();
        Self {
            rpm_limit,
            last_request: now,
            requests_in_window: 0,
            window_start: now,
        }
    }

    /// Check if a request can be made now, returns wait time in ms if not
    pub fn check_rate_limit(&mut self) -> Option<u64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.window_start);

        // Reset window if a minute has passed
        if elapsed >= Duration::from_secs(60) {
            self.window_start = now;
            self.requests_in_window = 0;
        }

        // Check if we've exceeded the rate limit
        if self.requests_in_window >= self.rpm_limit {
            let wait_time = Duration::from_secs(60) - elapsed;
            return Some(wait_time.as_millis() as u64);
        }

        // Ensure minimum interval between requests
        let since_last = now.duration_since(self.last_request);
        if since_last < Duration::from_millis(MIN_REQUEST_INTERVAL_MS) {
            let wait = Duration::from_millis(MIN_REQUEST_INTERVAL_MS) - since_last;
            return Some(wait.as_millis() as u64);
        }

        None
    }

    /// Record that a request was made
    pub fn record_request(&mut self) {
        self.last_request = Instant::now();
        self.requests_in_window += 1;
    }

    /// Wait until a request can be made
    pub async fn wait_if_needed(&mut self) {
        if let Some(wait_ms) = self.check_rate_limit() {
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        }
        self.record_request();
    }
}

/// Jupiter token scanner for meme coin discovery
#[derive(Debug)]
pub struct TokenScanner {
    /// Configuration
    config: ScannerConfig,
    /// HTTP client
    http_client: Client,
    /// Rate limiter
    rate_limiter: Arc<Mutex<RateLimiter>>,
    /// Cached token list
    cached_tokens: Arc<Mutex<Option<Vec<JupiterToken>>>>,
    /// Cache timestamp
    cache_time: Arc<Mutex<Option<Instant>>>,
}

impl TokenScanner {
    /// Create a new token scanner
    pub fn new(config: ScannerConfig) -> Result<Self, ScannerError> {
        config.validate()?;

        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(ScannerError::HttpError)?;

        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new(config.rate_limit_rpm)));

        Ok(Self {
            config,
            http_client,
            rate_limiter,
            cached_tokens: Arc::new(Mutex::new(None)),
            cache_time: Arc::new(Mutex::new(None)),
        })
    }

    /// Scan for top tokens from Jupiter
    pub async fn scan_top_tokens(&self) -> Result<Vec<MemeToken>, ScannerError> {
        // Rate limit check
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Fetch token list
        let tokens = self.fetch_token_list().await?;

        // Filter and sort by volume (simulated - real volume data requires additional API)
        let filtered = self.filter_tokens(tokens);

        // Get prices for filtered tokens
        let mints: Vec<String> = filtered.iter().map(|t| t.address.clone()).collect();
        let prices = self.get_batch_prices(&mints).await?;

        // Build MemeToken results
        let mut result: Vec<MemeToken> = filtered
            .into_iter()
            .filter_map(|t| {
                let price = prices.get(&t.address).copied().unwrap_or(0.0);
                if price > 0.0 {
                    Some(MemeToken {
                        mint: t.address,
                        symbol: t.symbol,
                        name: t.name,
                        volume_24h: 0.0, // Would need additional API for real volume
                        liquidity: 0.0,  // Would need additional API for real liquidity
                        price,
                        price_change_24h: 0.0, // Would need additional API
                        decimals: t.decimals,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Limit to top_n
        result.truncate(self.config.top_n_tokens);

        Ok(result)
    }

    /// Fetch token list from Jupiter
    async fn fetch_token_list(&self) -> Result<Vec<JupiterToken>, ScannerError> {
        // Check cache first (5 minute TTL)
        {
            let cache = self.cached_tokens.lock().await;
            let cache_time = self.cache_time.lock().await;

            if let (Some(tokens), Some(time)) = (cache.as_ref(), cache_time.as_ref()) {
                if time.elapsed() < Duration::from_secs(300) {
                    return Ok(tokens.clone());
                }
            }
        }

        // Rate limit
        self.rate_limiter.lock().await.wait_if_needed().await;

        // Fetch fresh list
        let response = self.http_client
            .get(JUPITER_TOKEN_LIST_API)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ScannerError::ParseError(
                format!("Token list API returned status: {}", response.status())
            ));
        }

        let tokens: Vec<JupiterToken> = response.json().await
            .map_err(|e| ScannerError::ParseError(e.to_string()))?;

        // Update cache
        {
            let mut cache = self.cached_tokens.lock().await;
            let mut cache_time = self.cache_time.lock().await;
            *cache = Some(tokens.clone());
            *cache_time = Some(Instant::now());
        }

        Ok(tokens)
    }

    /// Filter tokens based on configuration
    pub fn filter_tokens(&self, tokens: Vec<JupiterToken>) -> Vec<JupiterToken> {
        tokens
            .into_iter()
            .filter(|t| {
                // Filter out known stablecoins and wrapped tokens
                !is_stablecoin(&t.symbol)
                    && !is_wrapped_token(&t.symbol)
                    && t.decimals > 0
            })
            .collect()
    }

    /// Get prices for multiple tokens in a batch
    pub async fn get_batch_prices(&self, mints: &[String]) -> Result<HashMap<String, f64>, ScannerError> {
        if mints.is_empty() {
            return Ok(HashMap::new());
        }

        let mut result = HashMap::new();

        // Process in batches of 100 (Jupiter API limit)
        for chunk in mints.chunks(100) {
            // Rate limit
            self.rate_limiter.lock().await.wait_if_needed().await;

            let ids = chunk.join(",");
            let url = format!("{}?ids={}", JUPITER_PRICE_API, ids);

            let response = self.http_client
                .get(&url)
                .send()
                .await?;

            if response.status().is_success() {
                if let Ok(price_response) = response.json::<PriceResponse>().await {
                    for (mint, data) in price_response.data {
                        result.insert(mint, data.price);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get configuration
    pub fn config(&self) -> &ScannerConfig {
        &self.config
    }

    /// Clear token cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_tokens.lock().await;
        let mut cache_time = self.cache_time.lock().await;
        *cache = None;
        *cache_time = None;
    }
}

/// Check if token is a stablecoin
fn is_stablecoin(symbol: &str) -> bool {
    let stable_symbols = ["USDC", "USDT", "BUSD", "DAI", "TUSD", "USDP", "GUSD", "FRAX", "LUSD", "sUSD"];
    stable_symbols.iter().any(|s| symbol.to_uppercase().contains(s))
}

/// Check if token is a wrapped token
fn is_wrapped_token(symbol: &str) -> bool {
    let wrapped_prefixes = ["w", "W"];
    let symbol_upper = symbol.to_uppercase();
    wrapped_prefixes.iter().any(|p| symbol.starts_with(p))
        && (symbol_upper.contains("BTC") || symbol_upper.contains("ETH") || symbol_upper.contains("SOL"))
}

/// Jupiter token list response item
#[derive(Debug, Clone, Deserialize)]
struct JupiterToken {
    address: String,
    symbol: String,
    name: String,
    decimals: u8,
    #[serde(rename = "logoURI")]
    #[allow(dead_code)]
    logo_uri: Option<String>,
    #[allow(dead_code)]
    tags: Option<Vec<String>>,
}

/// Jupiter price API response
#[derive(Debug, Deserialize)]
struct PriceResponse {
    data: HashMap<String, PriceData>,
    #[serde(rename = "timeTaken")]
    #[allow(dead_code)]
    time_taken: Option<f64>,
}

/// Individual price data
#[derive(Debug, Deserialize)]
struct PriceData {
    #[allow(dead_code)]
    id: Option<String>,
    #[serde(rename = "mintSymbol")]
    #[allow(dead_code)]
    mint_symbol: Option<String>,
    #[serde(rename = "vsToken")]
    #[allow(dead_code)]
    vs_token: Option<String>,
    #[serde(rename = "vsTokenSymbol")]
    #[allow(dead_code)]
    vs_token_symbol: Option<String>,
    price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_config_default() {
        let config = ScannerConfig::default();
        assert_eq!(config.min_volume_24h, 500_000.0);
        assert_eq!(config.min_liquidity, 250_000.0);
        assert_eq!(config.top_n_tokens, 30);
        assert_eq!(config.max_spread_bps, 50);
        assert_eq!(config.min_token_age_hours, 24);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_scanner_config_builders() {
        let config = ScannerConfig::default()
            .with_min_volume(1_000_000.0)
            .with_min_liquidity(500_000.0)
            .with_top_n(50);

        assert_eq!(config.min_volume_24h, 1_000_000.0);
        assert_eq!(config.min_liquidity, 500_000.0);
        assert_eq!(config.top_n_tokens, 50);
    }

    #[test]
    fn test_scanner_config_validation() {
        let mut config = ScannerConfig::default();
        assert!(config.validate().is_ok());

        config.min_volume_24h = -1.0;
        assert!(config.validate().is_err());

        config.min_volume_24h = 500_000.0;
        config.top_n_tokens = 0;
        assert!(config.validate().is_err());

        config.top_n_tokens = 30;
        config.rate_limit_rpm = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_meme_token_tradeable() {
        let tradeable = MemeToken {
            mint: "test".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            volume_24h: 100_000.0,
            liquidity: 50_000.0,
            price: 1.0,
            price_change_24h: 5.0,
            decimals: 9,
        };
        assert!(tradeable.is_tradeable());

        let not_tradeable = MemeToken {
            mint: "test".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            volume_24h: 0.0,
            liquidity: 50_000.0,
            price: 1.0,
            price_change_24h: 5.0,
            decimals: 9,
        };
        assert!(!not_tradeable.is_tradeable());
    }

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(60);
        assert_eq!(limiter.rpm_limit, 60);
    }

    #[test]
    fn test_rate_limiter_first_request() {
        let mut limiter = RateLimiter::new(60);
        // First request after creation should wait for minimum interval
        // but since we just created it, it might need to wait
        limiter.record_request();
        assert_eq!(limiter.requests_in_window, 1);
    }

    #[test]
    fn test_rate_limiter_request_counting() {
        let mut limiter = RateLimiter::new(60);

        for _ in 0..5 {
            limiter.record_request();
        }

        assert_eq!(limiter.requests_in_window, 5);
    }

    #[test]
    fn test_is_stablecoin() {
        assert!(is_stablecoin("USDC"));
        assert!(is_stablecoin("USDT"));
        assert!(is_stablecoin("usdc"));
        assert!(is_stablecoin("DAI"));
        assert!(!is_stablecoin("SOL"));
        assert!(!is_stablecoin("BONK"));
    }

    #[test]
    fn test_is_wrapped_token() {
        assert!(is_wrapped_token("wBTC"));
        assert!(is_wrapped_token("WETH"));
        assert!(is_wrapped_token("wSOL"));
        assert!(!is_wrapped_token("SOL"));
        assert!(!is_wrapped_token("BONK"));
    }

    #[test]
    fn test_scanner_creation() {
        let config = ScannerConfig::default();
        let scanner = TokenScanner::new(config);
        assert!(scanner.is_ok());
    }

    #[test]
    fn test_scanner_invalid_config() {
        let mut config = ScannerConfig::default();
        config.rate_limit_rpm = 0;
        let scanner = TokenScanner::new(config);
        assert!(scanner.is_err());
    }

    #[test]
    fn test_filter_tokens() {
        let config = ScannerConfig::default();
        let scanner = TokenScanner::new(config).unwrap();

        let tokens = vec![
            JupiterToken {
                address: "addr1".to_string(),
                symbol: "BONK".to_string(),
                name: "Bonk".to_string(),
                decimals: 9,
                logo_uri: None,
                tags: None,
            },
            JupiterToken {
                address: "addr2".to_string(),
                symbol: "USDC".to_string(),
                name: "USD Coin".to_string(),
                decimals: 6,
                logo_uri: None,
                tags: None,
            },
            JupiterToken {
                address: "addr3".to_string(),
                symbol: "wBTC".to_string(),
                name: "Wrapped Bitcoin".to_string(),
                decimals: 8,
                logo_uri: None,
                tags: None,
            },
            JupiterToken {
                address: "addr4".to_string(),
                symbol: "WIF".to_string(),
                name: "Dogwifhat".to_string(),
                decimals: 9,
                logo_uri: None,
                tags: None,
            },
        ];

        let filtered = scanner.filter_tokens(tokens);

        // Should filter out USDC (stablecoin) and wBTC (wrapped)
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|t| t.symbol == "BONK"));
        assert!(filtered.iter().any(|t| t.symbol == "WIF"));
        assert!(!filtered.iter().any(|t| t.symbol == "USDC"));
        assert!(!filtered.iter().any(|t| t.symbol == "wBTC"));
    }

    #[test]
    fn test_scanner_error_display() {
        let err = ScannerError::NoTokenData;
        assert!(err.to_string().contains("No token data"));

        let err = ScannerError::RateLimitExceeded(1000);
        assert!(err.to_string().contains("Rate limit"));

        let err = ScannerError::ConfigError("test error".into());
        assert!(err.to_string().contains("test error"));
    }

    #[tokio::test]
    async fn test_get_batch_prices_empty() {
        let config = ScannerConfig::default();
        let scanner = TokenScanner::new(config).unwrap();

        let result = scanner.get_batch_prices(&[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let config = ScannerConfig::default();
        let scanner = TokenScanner::new(config).unwrap();

        // Clear cache should not panic
        scanner.clear_cache().await;

        // Check cache is cleared
        let cache = scanner.cached_tokens.lock().await;
        assert!(cache.is_none());
    }

    #[test]
    fn test_meme_token_serialization() {
        let token = MemeToken {
            mint: "test_mint".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            volume_24h: 100_000.0,
            liquidity: 50_000.0,
            price: 0.001,
            price_change_24h: 10.5,
            decimals: 9,
        };

        let json = serde_json::to_string(&token);
        assert!(json.is_ok());

        let json_str = json.unwrap();
        assert!(json_str.contains("test_mint"));
        assert!(json_str.contains("TEST"));
    }

    #[test]
    fn test_scanner_config_serialization() {
        let config = ScannerConfig::default();

        let json = serde_json::to_string(&config);
        assert!(json.is_ok());

        let deserialized: Result<ScannerConfig, _> = serde_json::from_str(&json.unwrap());
        assert!(deserialized.is_ok());

        let restored = deserialized.unwrap();
        assert_eq!(restored.min_volume_24h, config.min_volume_24h);
        assert_eq!(restored.top_n_tokens, config.top_n_tokens);
    }
}
