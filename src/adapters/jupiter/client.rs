//! Jupiter API Client
//!
//! HTTP client for Jupiter DEX aggregator V6 API.
//! Handles quote fetching, swap building, and transaction execution.

use std::time::Duration;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::ports::execution::{
    ExecutionPort, ExecutionError, SwapQuoteRequest, SwapQuoteResponse,
    ExecuteSwapRequest, ExecuteSwapResponse,
};
use super::quote::{QuoteRequest, QuoteResponse};
use super::swap::{SwapRequest, SwapResponse};

/// Jupiter API client configuration
#[derive(Debug, Clone)]
pub struct JupiterConfig {
    /// Base URL for Jupiter API
    pub api_base_url: String,
    /// Optional API key for higher rate limits
    pub api_key: Option<String>,
    /// Request timeout
    pub timeout: Duration,
    /// Number of retry attempts
    pub max_retries: u32,
}

impl Default for JupiterConfig {
    fn default() -> Self {
        Self {
            // Jupiter V1 API - requires API key for higher rate limits
            // https://api.jup.ag/swap/v1 (with API key)
            // lite-api.jup.ag deprecated Jan 31, 2026
            api_base_url: "https://api.jup.ag/swap/v1".to_string(),
            api_key: None,
            timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// Jupiter DEX aggregator client
#[derive(Debug, Clone)]
pub struct JupiterClient {
    config: JupiterConfig,
    http: Client,
}

impl JupiterClient {
    /// Create a new Jupiter client with default configuration
    pub fn new() -> Result<Self, ExecutionError> {
        Self::with_config(JupiterConfig::default())
    }

    /// Create a new Jupiter client with custom configuration
    pub fn with_config(config: JupiterConfig) -> Result<Self, ExecutionError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ExecutionError::ApiError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, http })
    }

    /// Create a new Jupiter client with API key
    pub fn with_api_key(api_key: String) -> Result<Self, ExecutionError> {
        let mut config = JupiterConfig::default();
        config.api_key = Some(api_key);
        Self::with_config(config)
    }

    /// Get a quote for a token swap
    pub async fn get_quote(&self, request: &QuoteRequest) -> Result<QuoteResponse, ExecutionError> {
        let url = format!("{}/quote", self.config.api_base_url);

        let mut req = self.http
            .get(&url)
            .query(&[
                ("inputMint", &request.input_mint),
                ("outputMint", &request.output_mint),
                ("amount", &request.amount.to_string()),
                ("slippageBps", &request.slippage_bps.to_string()),
            ]);

        if request.only_direct_routes {
            req = req.query(&[("onlyDirectRoutes", "true")]);
        }

        if let Some(ref api_key) = self.config.api_key {
            req = req.header("x-api-key", api_key);
        }

        let response: reqwest::Response = self.execute_with_retry(|| async {
            req.try_clone()
                .ok_or_else(|| ExecutionError::ApiError("Failed to clone request".into()))?
                .send()
                .await
                .map_err(|e| ExecutionError::ApiError(e.to_string()))
        }).await?;

        self.handle_response(response).await
    }

    /// Build and get swap transaction
    pub async fn get_swap_transaction(
        &self,
        request: &SwapRequest,
    ) -> Result<SwapResponse, ExecutionError> {
        let url = format!("{}/swap", self.config.api_base_url);

        let mut req = self.http
            .post(&url)
            .json(request);

        if let Some(ref api_key) = self.config.api_key {
            req = req.header("x-api-key", api_key);
        }

        let response: reqwest::Response = self.execute_with_retry(|| async {
            req.try_clone()
                .ok_or_else(|| ExecutionError::ApiError("Failed to clone request".into()))?
                .send()
                .await
                .map_err(|e| ExecutionError::ApiError(e.to_string()))
        }).await?;

        self.handle_response(response).await
    }

    /// Execute request with retry logic and rate limit handling
    async fn execute_with_retry<F, Fut>(&self, request_fn: F) -> Result<reqwest::Response, ExecutionError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response, ExecutionError>>,
    {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            match request_fn().await {
                Ok(response) => {
                    if response.status().is_success() || response.status() == StatusCode::BAD_REQUEST {
                        return Ok(response);
                    }

                    // Handle rate limiting (429) with exponential backoff
                    if response.status() == StatusCode::TOO_MANY_REQUESTS {
                        let backoff = Duration::from_secs(2u64.pow(attempt + 1)); // 2s, 4s, 8s
                        tracing::warn!(
                            "Rate limited (429), backing off for {:?} (attempt {}/{})",
                            backoff, attempt + 1, self.config.max_retries
                        );
                        last_error = Some(ExecutionError::ApiError(
                            "Rate limit exceeded - backing off".into()
                        ));
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // Retry on server errors (5xx)
                    if response.status().is_server_error() {
                        last_error = Some(ExecutionError::ApiError(
                            format!("Server error: {}", response.status())
                        ));
                        tokio::time::sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                        continue;
                    }

                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);
                    tokio::time::sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ExecutionError::ApiError("Max retries exceeded".into())))
    }

    /// Handle API response and deserialize
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, ExecutionError> {
        let status = response.status();

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(ExecutionError::ApiError("Rate limit exceeded".into()));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();

            // Check for slippage error
            if error_text.contains("SlippageToleranceExceeded") || error_text.contains("6001") {
                return Err(ExecutionError::SlippageExceeded);
            }

            return Err(ExecutionError::ApiError(format!(
                "API error {}: {}",
                status,
                error_text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| ExecutionError::ApiError(format!("Failed to parse response: {}", e)))
    }

    /// Get the configured API base URL
    pub fn api_base_url(&self) -> &str {
        &self.config.api_base_url
    }
}

impl Default for JupiterClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default JupiterClient")
    }
}

#[async_trait]
impl ExecutionPort for JupiterClient {
    async fn get_swap_quote(
        &self,
        request: SwapQuoteRequest,
    ) -> Result<SwapQuoteResponse, ExecutionError> {
        let quote_request = QuoteRequest {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount: request.amount,
            slippage_bps: request.slippage_bps,
            only_direct_routes: false,
            restrict_intermediate_tokens: None,
            platform_fee_bps: None,
        };

        let quote = self.get_quote(&quote_request).await?;

        Ok(SwapQuoteResponse {
            input_amount: quote.in_amount.parse().unwrap_or(0),
            output_amount: quote.out_amount.parse().unwrap_or(0),
            min_output_amount: quote.other_amount_threshold.parse().unwrap_or(0),
            transaction: String::new(), // Transaction comes from swap endpoint
            route: quote.route_plan.iter()
                .map(|r| r.swap_info.label.clone())
                .collect(),
        })
    }

    async fn execute_swap(
        &self,
        request: ExecuteSwapRequest,
    ) -> Result<ExecuteSwapResponse, ExecutionError> {
        // Build swap request from quote response
        let swap_request = SwapRequest {
            user_public_key: request.user_public_key.clone(),
            quote_response: serde_json::to_value(&request.quote_response)
                .map_err(|e| ExecutionError::InvalidParameters(e.to_string()))?,
            prioritization_fee_lamports: request.prioritization_fee_lamports,
            dynamic_compute_unit_limit: request.dynamic_compute_unit_limit.unwrap_or(true),
        };

        let _swap = self.get_swap_transaction(&swap_request).await?;

        Ok(ExecuteSwapResponse {
            signature: String::new(), // Signature comes after signing
            status: "pending".to_string(),
            output_amount: request.quote_response.output_amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jupiter_config_default() {
        let config = JupiterConfig::default();
        assert_eq!(config.api_base_url, "https://api.jup.ag/swap/v1");
        assert!(config.api_key.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_jupiter_client_creation() {
        let client = JupiterClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_jupiter_client_with_api_key() {
        let client = JupiterClient::with_api_key("test-key".to_string());
        assert!(client.is_ok());
    }
}
