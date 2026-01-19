use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("API request failed: {0}")]
    ApiError(String),
    #[error("Transaction signing failed: {0}")]
    SigningError(String),
    #[error("Transaction execution failed: {0}")]
    ExecutionError(String),
    #[error("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapQuoteRequest {
    pub input_mint: String,
    pub output_mint: String,
    pub amount: u64,
    pub slippage_bps: u16, // basis points (1 = 0.01%)
    pub platform_fee_bps: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SwapQuoteResponse {
    pub input_amount: u64,
    pub output_amount: u64,
    pub min_output_amount: u64, // after slippage
    pub transaction: String,    // base64 encoded
    pub route: Vec<String>,     // token route
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteSwapRequest {
    pub quote_response: SwapQuoteResponse,
    pub user_public_key: String,
    pub prioritization_fee_lamports: Option<u64>,
    pub dynamic_compute_unit_limit: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteSwapResponse {
    pub signature: String,
    pub status: String,
    pub output_amount: u64,
}

#[async_trait::async_trait]
pub trait ExecutionPort {
    async fn get_swap_quote(
        &self,
        request: SwapQuoteRequest,
    ) -> Result<SwapQuoteResponse, ExecutionError>;

    async fn execute_swap(
        &self,
        request: ExecuteSwapRequest,
    ) -> Result<ExecuteSwapResponse, ExecutionError>;

    async fn build_and_execute_swap(
        &self,
        quote_request: SwapQuoteRequest,
        execution_request: ExecuteSwapRequest,
    ) -> Result<ExecuteSwapResponse, ExecutionError> {
        let quote = self.get_swap_quote(quote_request).await?;
        let request = ExecuteSwapRequest {
            quote_response: quote,
            ..execution_request
        };
        self.execute_swap(request).await
    }
}

pub struct JupiterExecutionPort {
    api_base_url: String,
    api_key: Option<String>,
}

impl JupiterExecutionPort {
    pub fn new(api_base_url: String, api_key: Option<String>) -> Self {
        Self {
            api_base_url,
            api_key,
        }
    }
}

#[async_trait::async_trait]
impl ExecutionPort for JupiterExecutionPort {
    async fn get_swap_quote(
        &self,
        request: SwapQuoteRequest,
    ) -> Result<SwapQuoteResponse, ExecutionError> {
        let client = reqwest::Client::new();
        let url = format!("{}/quote", self.api_base_url);

        let response = client
            .get(&url)
            .query(&request)
            .header(
                "x-api-key",
                self.api_key.as_ref().ok_or_else(|| {
                    ExecutionError::InvalidParameters("API key required".to_string())
                })?,
            )
            .send()
            .await
            .map_err(|e| ExecutionError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ExecutionError::ApiError(
                response.text().await.unwrap_or_default(),
            ));
        }

        response
            .json()
            .await
            .map_err(|e| ExecutionError::ApiError(e.to_string()))
    }

    async fn execute_swap(
        &self,
        request: ExecuteSwapRequest,
    ) -> Result<ExecuteSwapResponse, ExecutionError> {
        let client = reqwest::Client::new();
        let url = format!("{}/swap", self.api_base_url);

        let response = client
            .post(&url)
            .json(&request)
            .header(
                "x-api-key",
                self.api_key.as_ref().ok_or_else(|| {
                    ExecutionError::InvalidParameters("API key required".to_string())
                })?,
            )
            .send()
            .await
            .map_err(|e| ExecutionError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ExecutionError::ApiError(
                response.text().await.unwrap_or_default(),
            ));
        }

        response
            .json()
            .await
            .map_err(|e| ExecutionError::ApiError(e.to_string()))
    }
}