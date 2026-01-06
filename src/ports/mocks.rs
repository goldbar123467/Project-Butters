use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;

/// Mock market data port that records calls and allows controlled responses
#[derive(Debug, Default)]
pub struct MockMarketData {
    calls: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<HashMap<String, String>>>,
}

impl MockMarketData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set a response for a given symbol
    pub fn with_response(mut self, symbol: &str, response: &str) -> Self {
        self.responses.lock().unwrap().insert(symbol.to_string(), response.to_string());
        self
    }

    /// Get all recorded calls
    pub fn get_calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
pub trait MarketDataPort {
    async fn get_price(&self, symbol: &str) -> Result<String, String>;
}

#[async_trait]
impl MarketDataPort for MockMarketData {
    async fn get_price(&self, symbol: &str) -> Result<String, String> {
        self.calls.lock().unwrap().push(symbol.to_string());
        self.responses
            .lock()
            .unwrap()
            .get(symbol)
            .cloned()
            .ok_or_else(|| "No response configured".to_string())
    }
}

/// Mock execution port that records calls and allows controlled responses
#[derive(Debug, Default)]
pub struct MockExecution {
    calls: Arc<Mutex<Vec<(String, f64)>>>,
    responses: Arc<Mutex<HashMap<String, bool>>>,
}

impl MockExecution {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set a response for a given order
    pub fn with_response(mut self, order_id: &str, success: bool) -> Self {
        self.responses.lock().unwrap().insert(order_id.to_string(), success);
        self
    }

    /// Get all recorded calls
    pub fn get_calls(&self) -> Vec<(String, f64)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
pub trait ExecutionPort {
    async fn execute_order(&self, order_id: &str, amount: f64) -> Result<bool, String>;
}

#[async_trait]
impl ExecutionPort for MockExecution {
    async fn execute_order(&self, order_id: &str, amount: f64) -> Result<bool, String> {
        self.calls.lock().unwrap().push((order_id.to_string(), amount));
        self.responses
            .lock()
            .unwrap()
            .get(order_id)
            .copied()
            .ok_or_else(|| "No response configured".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_market_data() {
        let mock = MockMarketData::new()
            .with_response("AAPL", "150.0");

        let result = mock.get_price("AAPL").await;
        assert_eq!(result, Ok("150.0".to_string()));
        assert_eq!(mock.get_calls(), vec!["AAPL".to_string()]);
    }

    #[tokio::test]
    async fn test_mock_execution() {
        let mock = MockExecution::new()
            .with_response("order123", true);

        let result = mock.execute_order("order123", 100.0).await;
        assert_eq!(result, Ok(true));
        assert_eq!(mock.get_calls(), vec![("order123".to_string(), 100.0)]);
    }
}