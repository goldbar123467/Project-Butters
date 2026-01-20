//! Jupiter Token List Fetcher
//!
//! Fetches token information and prices from Jupiter's Token API V2 and Price API V3.
//!
//! # Endpoints Used
//! - Token API V2: `https://lite-api.jup.ag/tokens/v2` (free, deprecated Jan 31 2026)
//! - Price API V3: `https://lite-api.jup.ag/price/v3` (free, rate limited)
//!
//! # Features
//! - Fetch token info by mint address
//! - Search tokens by name/symbol
//! - Get verified token list
//! - Get trending/top traded tokens
//! - Get token prices (single or batch)

use std::collections::HashMap;
use std::time::Duration;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when fetching token data
#[derive(Debug, Error)]
pub enum JupiterTokenError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("Token not found: {0}")]
    TokenNotFound(String),

    #[error("No price data for mint: {0}")]
    NoPriceData(String),

    #[error("Rate limited, try again later")]
    RateLimited,

    #[error("API error: {0}")]
    ApiError(String),
}

/// Configuration for the Jupiter token fetcher
#[derive(Debug, Clone)]
pub struct JupiterTokenConfig {
    /// Base URL for Token API V2
    pub token_api_url: String,
    /// Base URL for Price API V3
    pub price_api_url: String,
    /// Optional API key for higher rate limits
    pub api_key: Option<String>,
    /// Request timeout
    pub timeout: Duration,
    /// Number of retry attempts
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds)
    pub retry_base_delay_ms: u64,
}

impl Default for JupiterTokenConfig {
    fn default() -> Self {
        Self {
            // lite-api.jup.ag deprecating Jan 31, 2026
            // Use api.jup.ag with API key for production
            token_api_url: "https://lite-api.jup.ag/tokens/v2".to_string(),
            price_api_url: "https://lite-api.jup.ag/price/v3".to_string(),
            api_key: None,
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_base_delay_ms: 500,
        }
    }
}

impl JupiterTokenConfig {
    /// Create config with API key (uses api.jup.ag for higher limits)
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        Self {
            token_api_url: "https://api.jup.ag/tokens/v2".to_string(),
            price_api_url: "https://api.jup.ag/price/v3".to_string(),
            api_key: Some(api_key),
            ..Default::default()
        }
    }
}

/// Token information from Jupiter API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterToken {
    /// Token mint address
    pub address: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Number of decimals
    pub decimals: u8,
    /// Logo URI (optional)
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    /// Tags (e.g., "verified", "strict", "community")
    #[serde(default)]
    pub tags: Vec<String>,
    /// Daily volume (if available)
    #[serde(rename = "daily_volume")]
    pub daily_volume: Option<f64>,
}

impl JupiterToken {
    /// Check if token is verified by Jupiter
    pub fn is_verified(&self) -> bool {
        self.tags.iter().any(|t| t == "verified")
    }

    /// Check if token passes strict verification
    pub fn is_strict(&self) -> bool {
        self.tags.iter().any(|t| t == "strict")
    }

    /// Check if token is community verified
    pub fn is_community(&self) -> bool {
        self.tags.iter().any(|t| t == "community")
    }
}

/// Token price information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    /// Token mint address
    pub mint: String,
    /// Price in USD
    pub price: f64,
    /// Price type (e.g., "derivedPrice")
    pub price_type: String,
    /// Timestamp of price fetch
    pub timestamp: u64,
}

/// Price API V3 response
#[derive(Debug, Clone, Deserialize)]
struct PriceV3Response {
    data: HashMap<String, PriceV3Data>,
    #[serde(rename = "timeTaken")]
    #[allow(dead_code)]
    time_taken: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PriceV3Data {
    id: String,
    #[serde(rename = "type")]
    price_type: String,
    price: String,
}

/// Token search response
#[derive(Debug, Clone, Deserialize)]
struct TokenSearchResponse {
    #[serde(default)]
    tokens: Vec<JupiterToken>,
}

/// Trending tokens query interval
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendingInterval {
    FiveMinutes,
    OneHour,
    SixHours,
    TwentyFourHours,
}

impl TrendingInterval {
    fn as_str(&self) -> &'static str {
        match self {
            TrendingInterval::FiveMinutes => "5m",
            TrendingInterval::OneHour => "1h",
            TrendingInterval::SixHours => "6h",
            TrendingInterval::TwentyFourHours => "24h",
        }
    }
}

/// Category for trending/top tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCategory {
    /// Top trending tokens
    TopTrending,
    /// Top traded by volume
    TopTraded,
    /// Top organic score
    TopOrganicScore,
}

impl TokenCategory {
    fn as_str(&self) -> &'static str {
        match self {
            TokenCategory::TopTrending => "toptrending",
            TokenCategory::TopTraded => "toptraded",
            TokenCategory::TopOrganicScore => "toporganicscore",
        }
    }
}

/// Jupiter Token List Fetcher
#[derive(Debug, Clone)]
pub struct JupiterTokenFetcher {
    config: JupiterTokenConfig,
    http: Client,
}

impl JupiterTokenFetcher {
    /// Create a new fetcher with default configuration
    pub fn new() -> Result<Self, JupiterTokenError> {
        Self::with_config(JupiterTokenConfig::default())
    }

    /// Create a new fetcher with API key
    pub fn with_api_key(api_key: impl Into<String>) -> Result<Self, JupiterTokenError> {
        Self::with_config(JupiterTokenConfig::with_api_key(api_key))
    }

    /// Create a new fetcher with custom configuration
    pub fn with_config(config: JupiterTokenConfig) -> Result<Self, JupiterTokenError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()?;

        Ok(Self { config, http })
    }

    /// Get token info by mint address
    pub async fn get_token(&self, mint: &str) -> Result<JupiterToken, JupiterTokenError> {
        let url = format!("{}/token/{}", self.config.token_api_url, mint);
        let response = self.execute_request(&url).await?;

        response.json::<JupiterToken>().await.map_err(|e| {
            if e.to_string().contains("404") || e.to_string().contains("null") {
                JupiterTokenError::TokenNotFound(mint.to_string())
            } else {
                JupiterTokenError::ParseError(format!("Failed to parse token: {}", e))
            }
        })
    }

    /// Search tokens by name or symbol
    pub async fn search_tokens(&self, query: &str) -> Result<Vec<JupiterToken>, JupiterTokenError> {
        let url = format!("{}/search?query={}", self.config.token_api_url, query);
        let response = self.execute_request(&url).await?;

        // The API returns an array directly
        let tokens: Vec<JupiterToken> = response.json().await.map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse search results: {}", e))
        })?;

        Ok(tokens)
    }

    /// Get list of verified token mints
    pub async fn get_verified_tokens(&self) -> Result<Vec<String>, JupiterTokenError> {
        let url = format!("{}/tag?query=verified", self.config.token_api_url);
        let response = self.execute_request(&url).await?;

        // Returns array of mint addresses
        let mints: Vec<String> = response.json().await.map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse verified tokens: {}", e))
        })?;

        Ok(mints)
    }

    /// Check if a token is verified
    pub async fn is_verified(&self, mint: &str) -> Result<bool, JupiterTokenError> {
        match self.get_token(mint).await {
            Ok(token) => Ok(token.is_verified()),
            Err(JupiterTokenError::TokenNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Get trending/top tokens by category
    pub async fn get_trending_tokens(
        &self,
        category: TokenCategory,
        interval: TrendingInterval,
    ) -> Result<Vec<JupiterToken>, JupiterTokenError> {
        let url = format!(
            "{}/category?query={}&interval={}",
            self.config.token_api_url,
            category.as_str(),
            interval.as_str()
        );
        let response = self.execute_request(&url).await?;

        let tokens: Vec<JupiterToken> = response.json().await.map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse trending tokens: {}", e))
        })?;

        Ok(tokens)
    }

    /// Get price for a single token (in USD)
    pub async fn get_price(&self, mint: &str) -> Result<TokenPrice, JupiterTokenError> {
        let url = format!("{}?ids={}", self.config.price_api_url, mint);
        let response = self.execute_request(&url).await?;

        let price_response: PriceV3Response = response.json().await.map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse price response: {}", e))
        })?;

        let price_data = price_response
            .data
            .get(mint)
            .ok_or_else(|| JupiterTokenError::NoPriceData(mint.to_string()))?;

        let price: f64 = price_data.price.parse().map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse price value: {}", e))
        })?;

        Ok(TokenPrice {
            mint: mint.to_string(),
            price,
            price_type: price_data.price_type.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Get prices for multiple tokens (batch request)
    pub async fn get_prices(
        &self,
        mints: &[&str],
    ) -> Result<HashMap<String, TokenPrice>, JupiterTokenError> {
        if mints.is_empty() {
            return Ok(HashMap::new());
        }

        let ids = mints.join(",");
        let url = format!("{}?ids={}", self.config.price_api_url, ids);
        let response = self.execute_request(&url).await?;

        let price_response: PriceV3Response = response.json().await.map_err(|e| {
            JupiterTokenError::ParseError(format!("Failed to parse prices response: {}", e))
        })?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut prices = HashMap::new();
        for (mint, data) in price_response.data {
            if let Ok(price) = data.price.parse::<f64>() {
                prices.insert(
                    mint.clone(),
                    TokenPrice {
                        mint,
                        price,
                        price_type: data.price_type,
                        timestamp,
                    },
                );
            }
        }

        Ok(prices)
    }

    /// Execute request with retry logic
    async fn execute_request(&self, url: &str) -> Result<reqwest::Response, JupiterTokenError> {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            let mut req = self.http.get(url);

            // Add API key header if configured
            if let Some(ref api_key) = self.config.api_key {
                req = req.header("x-api-key", api_key);
            }

            match req.send().await {
                Ok(response) => {
                    let status = response.status();

                    // Handle rate limiting with exponential backoff
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        let backoff = Duration::from_millis(
                            self.config.retry_base_delay_ms * 2u64.pow(attempt + 1),
                        );
                        tracing::warn!(
                            "Rate limited (429), backing off for {:?} (attempt {}/{})",
                            backoff,
                            attempt + 1,
                            self.config.max_retries
                        );
                        last_error = Some(JupiterTokenError::RateLimited);
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // Retry on server errors (5xx)
                    if status.is_server_error() {
                        let backoff = Duration::from_millis(
                            self.config.retry_base_delay_ms * (attempt as u64 + 1),
                        );
                        last_error = Some(JupiterTokenError::ApiError(format!(
                            "Server error: {}",
                            status
                        )));
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // Handle 404 for token not found
                    if status == StatusCode::NOT_FOUND {
                        return Err(JupiterTokenError::TokenNotFound(url.to_string()));
                    }

                    // Handle other client errors
                    if status.is_client_error() {
                        let error_text = response.text().await.unwrap_or_default();
                        return Err(JupiterTokenError::ApiError(format!(
                            "API error {}: {}",
                            status, error_text
                        )));
                    }

                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(JupiterTokenError::HttpError(e));
                    let backoff = Duration::from_millis(
                        self.config.retry_base_delay_ms * (attempt as u64 + 1),
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            JupiterTokenError::ApiError("Max retries exceeded".into())
        }))
    }
}

impl Default for JupiterTokenFetcher {
    fn default() -> Self {
        Self::new().expect("Failed to create default JupiterTokenFetcher")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = JupiterTokenConfig::default();
        assert_eq!(config.token_api_url, "https://lite-api.jup.ag/tokens/v2");
        assert_eq!(config.price_api_url, "https://lite-api.jup.ag/price/v3");
        assert!(config.api_key.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_with_api_key() {
        let config = JupiterTokenConfig::with_api_key("test-key");
        assert_eq!(config.token_api_url, "https://api.jup.ag/tokens/v2");
        assert_eq!(config.price_api_url, "https://api.jup.ag/price/v3");
        assert_eq!(config.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_fetcher_creation() {
        let fetcher = JupiterTokenFetcher::new();
        assert!(fetcher.is_ok());
    }

    #[test]
    fn test_fetcher_with_api_key() {
        let fetcher = JupiterTokenFetcher::with_api_key("test-key");
        assert!(fetcher.is_ok());
    }

    #[test]
    fn test_jupiter_token_verification_flags() {
        let token = JupiterToken {
            address: "TestMint123".to_string(),
            name: "Test Token".to_string(),
            symbol: "TEST".to_string(),
            decimals: 9,
            logo_uri: None,
            tags: vec!["verified".to_string(), "strict".to_string()],
            daily_volume: None,
        };

        assert!(token.is_verified());
        assert!(token.is_strict());
        assert!(!token.is_community());
    }

    #[test]
    fn test_jupiter_token_not_verified() {
        let token = JupiterToken {
            address: "UnverifiedMint".to_string(),
            name: "Unverified Token".to_string(),
            symbol: "UNVER".to_string(),
            decimals: 6,
            logo_uri: None,
            tags: vec![],
            daily_volume: None,
        };

        assert!(!token.is_verified());
        assert!(!token.is_strict());
        assert!(!token.is_community());
    }

    #[test]
    fn test_trending_interval_as_str() {
        assert_eq!(TrendingInterval::FiveMinutes.as_str(), "5m");
        assert_eq!(TrendingInterval::OneHour.as_str(), "1h");
        assert_eq!(TrendingInterval::SixHours.as_str(), "6h");
        assert_eq!(TrendingInterval::TwentyFourHours.as_str(), "24h");
    }

    #[test]
    fn test_token_category_as_str() {
        assert_eq!(TokenCategory::TopTrending.as_str(), "toptrending");
        assert_eq!(TokenCategory::TopTraded.as_str(), "toptraded");
        assert_eq!(TokenCategory::TopOrganicScore.as_str(), "toporganicscore");
    }

    #[test]
    fn test_token_price_struct() {
        let price = TokenPrice {
            mint: "TestMint".to_string(),
            price: 150.50,
            price_type: "derivedPrice".to_string(),
            timestamp: 1700000000,
        };

        assert_eq!(price.mint, "TestMint");
        assert!((price.price - 150.50).abs() < 0.001);
    }

    #[test]
    fn test_default_fetcher() {
        let _fetcher = JupiterTokenFetcher::default();
    }
}
