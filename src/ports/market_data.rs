use async_trait::async_trait;
use thiserror::Error;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Market data error type
#[derive(Error, Debug)]
pub enum MarketDataError {
    #[error("WebSocket connection error: {0}")]
    WebSocketError(String),
    
    #[error("REST API error: {0}")]
    RestError(String),
    
    #[error("Data parsing error: {0}")]
    ParseError(String),
    
    #[error("Subscription error: {0}")]
    SubscriptionError(String),
    
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

/// OHLCV data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ohlcv {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Market data event type
#[derive(Debug, Clone)]
pub enum MarketDataEvent {
    Ohlcv(Ohlcv),
    Trade { price: f64, volume: f64, timestamp: DateTime<Utc> },
    OrderBookUpdate { bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)> },
}

/// Market data subscription parameters
#[derive(Debug, Clone)]
pub struct SubscriptionParams {
    pub symbol: String,
    pub interval: Option<String>,
    pub depth: Option<usize>,
}

/// Historical data query parameters
#[derive(Debug, Clone)]
pub struct HistoricalQuery {
    pub symbol: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub interval: String,
    pub limit: Option<usize>,
}

/// Market data port trait
#[async_trait]
pub trait MarketDataPort: Send + Sync {
    /// Subscribe to real-time market data
    /// Returns a channel receiver for market data events
    async fn subscribe(&self, params: SubscriptionParams) 
        -> Result<mpsc::Receiver<MarketDataEvent>, MarketDataError>;
    
    /// Fetch historical OHLCV data
    async fn fetch_historical(&self, query: HistoricalQuery) 
        -> Result<Vec<Ohlcv>, MarketDataError>;
    
    /// Unsubscribe from market data
    async fn unsubscribe(&self, symbol: &str) -> Result<(), MarketDataError>;
    
    /// Check connection status
    fn is_connected(&self) -> bool;
}

/// WebSocket market data implementation
pub struct WebSocketMarketData {
    // Implementation details would go here
}

#[async_trait]
impl MarketDataPort for WebSocketMarketData {
    async fn subscribe(&self, params: SubscriptionParams) 
        -> Result<mpsc::Receiver<MarketDataEvent>, MarketDataError> {
        // Implementation would connect to WebSocket and return receiver
        todo!()
    }
    
    async fn fetch_historical(&self, _query: HistoricalQuery) 
        -> Result<Vec<Ohlcv>, MarketDataError> {
        Err(MarketDataError::Unsupported("WebSocket implementation cannot fetch historical data".into()))
    }
    
    async fn unsubscribe(&self, _symbol: &str) -> Result<(), MarketDataError> {
        // Implementation would unsubscribe from WebSocket
        todo!()
    }
    
    fn is_connected(&self) -> bool {
        // Implementation would check WebSocket connection status
        todo!()
    }
}

/// REST market data implementation
pub struct RestMarketData {
    // Implementation details would go here
}

#[async_trait]
impl MarketDataPort for RestMarketData {
    async fn subscribe(&self, _params: SubscriptionParams) 
        -> Result<mpsc::Receiver<MarketDataEvent>, MarketDataError> {
        Err(MarketDataError::Unsupported("REST implementation cannot stream real-time data".into()))
    }
    
    async fn fetch_historical(&self, query: HistoricalQuery) 
        -> Result<Vec<Ohlcv>, MarketDataError> {
        // Implementation would fetch from REST API
        todo!()
    }
    
    async fn unsubscribe(&self, _symbol: &str) -> Result<(), MarketDataError> {
        Ok(())
    }
    
    fn is_connected(&self) -> bool {
        true
    }
}