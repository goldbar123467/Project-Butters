//! Common data structures and error types for all ports

use serde::{Deserialize, Serialize};
use thiserror::Error;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Common result type for port operations
pub type PortResult<T> = Result<T, PortError>;

/// Error hierarchy for port operations
#[derive(Error, Debug, Serialize, Deserialize)]
pub enum PortError {
    /// Invalid input data
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Network/communication error
    #[error("Communication error: {0}")]
    Communication(String),
    
    /// Protocol-specific error
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    /// Authentication/authorization error
    #[error("Authentication error: {0}")]
    Authentication(String),
}

/// Common instrument representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    /// Unique identifier for the instrument
    pub id: String,
    
    /// Symbol/ticker
    pub symbol: String,
    
    /// Instrument type (spot, future, option, etc.)
    pub instrument_type: InstrumentType,
    
    /// Base currency
    pub base_currency: String,
    
    /// Quote currency
    pub quote_currency: String,
    
    /// Minimum order size
    pub min_size: Decimal,
    
    /// Price increment
    pub price_increment: Decimal,
    
    /// Size increment
    pub size_increment: Decimal,
}

/// Instrument type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentType {
    Spot,
    Future,
    Option,
    Perpetual,
}

/// Price update structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    /// Instrument ID
    pub instrument_id: String,
    
    /// Timestamp of the price update
    pub timestamp: DateTime<Utc>,
    
    /// Best bid price
    pub bid: Decimal,
    
    /// Best ask price
    pub ask: Decimal,
    
    /// Last traded price
    pub last: Option<Decimal>,
}

/// Trade structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    /// Trade ID
    pub trade_id: String,
    
    /// Instrument ID
    pub instrument_id: String,
    
    /// Timestamp of the trade
    pub timestamp: DateTime<Utc>,
    
    /// Trade price
    pub price: Decimal,
    
    /// Trade size
    pub size: Decimal,
    
    /// Trade side (buy/sell)
    pub side: TradeSide,
}

/// Trade side (buy/sell)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Order structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    /// Order ID
    pub order_id: String,
    
    /// Instrument ID
    pub instrument_id: String,
    
    /// Order creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Order type (limit, market, etc.)
    pub order_type: OrderType,
    
    /// Order side (buy/sell)
    pub side: TradeSide,
    
    /// Order price (for limit orders)
    pub price: Option<Decimal>,
    
    /// Order size
    pub size: Decimal,
    
    /// Filled size
    pub filled: Decimal,
    
    /// Order status
    pub status: OrderStatus,
}

/// Order type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Limit,
    Market,
    Stop,
    StopLimit,
}

/// Order status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

// Serialization formats documentation
//
// All structures implement Serialize/Deserialize using JSON as the primary format.
// For binary protocols, we recommend:
// - MessagePack for compact binary representation
// - CBOR for self-describing binary format
// - BSON for MongoDB compatibility
//
// Timestamps are serialized as ISO 8601 strings in UTC.
// Decimal numbers are serialized as strings to avoid precision loss.
// Enums use snake_case naming convention in serialization.
