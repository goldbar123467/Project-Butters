use std::time::Duration;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

const JUPITER_PRICE_API: &str = "https://price.jup.ag/v6/price";

#[derive(Debug, Error)]
pub enum PriceError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("No price data for mint: {0}")]
    NoPriceData(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone)]
pub struct JupiterPriceClient {
    http: Client,
    timeout: Duration,
}

impl JupiterPriceClient {
    pub fn new() -> Result<Self, PriceError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { http, timeout: Duration::from_secs(10) })
    }

    /// Get price for a single token in USDC
    pub async fn get_price(&self, mint: &str) -> Result<f64, PriceError> {
        let url = format!("{}?ids={}", JUPITER_PRICE_API, mint);

        let response: PriceResponse = self.http
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        response.data
            .get(mint)
            .map(|p| p.price)
            .ok_or_else(|| PriceError::NoPriceData(mint.to_string()))
    }

    /// Get price of base token in terms of quote token
    pub async fn get_pair_price(&self, base_mint: &str, quote_mint: &str) -> Result<f64, PriceError> {
        let url = format!("{}?ids={},{}", JUPITER_PRICE_API, base_mint, quote_mint);

        let response: PriceResponse = self.http
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let base_price = response.data
            .get(base_mint)
            .map(|p| p.price)
            .ok_or_else(|| PriceError::NoPriceData(base_mint.to_string()))?;

        let quote_price = response.data
            .get(quote_mint)
            .map(|p| p.price)
            .ok_or_else(|| PriceError::NoPriceData(quote_mint.to_string()))?;

        // Return base in terms of quote
        Ok(base_price / quote_price)
    }
}

impl Default for JupiterPriceClient {
    fn default() -> Self {
        Self::new().expect("Failed to create JupiterPriceClient")
    }
}

#[derive(Debug, Deserialize)]
struct PriceResponse {
    data: std::collections::HashMap<String, PriceData>,
    #[serde(rename = "timeTaken")]
    #[allow(dead_code)]
    time_taken: f64,
}

#[derive(Debug, Deserialize)]
struct PriceData {
    #[allow(dead_code)]
    id: String,
    #[serde(rename = "mintSymbol")]
    #[allow(dead_code)]
    mint_symbol: String,
    #[serde(rename = "vsToken")]
    #[allow(dead_code)]
    vs_token: String,
    #[serde(rename = "vsTokenSymbol")]
    #[allow(dead_code)]
    vs_token_symbol: String,
    price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = JupiterPriceClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_default_client() {
        let _client = JupiterPriceClient::default();
    }
}
