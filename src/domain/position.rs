use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Side {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Open,
    Closed,
    Liquidated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub side: Side,
    pub status: Status,
    pub entry_price: f64,
    pub quantity: f64,
    pub token_pair: String,
}

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("Position is already closed")]
    AlreadyClosed,
    #[error("Position is already open")]
    AlreadyOpen,
    #[error("Invalid quantity: {0}")]
    InvalidQuantity(f64),
    #[error("Invalid entry price: {0}")]
    InvalidEntryPrice(f64),
}

impl Position {
    pub fn new(side: Side, entry_price: f64, quantity: f64, token_pair: String) -> Result<Self, PositionError> {
        if quantity <= 0.0 {
            return Err(PositionError::InvalidQuantity(quantity));
        }
        if entry_price <= 0.0 {
            return Err(PositionError::InvalidEntryPrice(entry_price));
        }

        Ok(Self {
            side,
            status: Status::Open,
            entry_price,
            quantity,
            token_pair,
        })
    }

    pub fn close(&mut self) -> Result<(), PositionError> {
        if self.status != Status::Open {
            return Err(PositionError::AlreadyClosed);
        }
        self.status = Status::Closed;
        Ok(())
    }

    pub fn update(&mut self, new_quantity: f64) -> Result<(), PositionError> {
        if self.status != Status::Open {
            return Err(PositionError::AlreadyClosed);
        }
        if new_quantity <= 0.0 {
            return Err(PositionError::InvalidQuantity(new_quantity));
        }
        self.quantity = new_quantity;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_position() {
        let position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        assert_eq!(position.side, Side::Long);
        assert_eq!(position.status, Status::Open);
        assert_eq!(position.entry_price, 100.0);
        assert_eq!(position.quantity, 1.0);
        assert_eq!(position.token_pair, "BTC/USD");
    }

    #[test]
    fn test_new_position_invalid_quantity() {
        let result = Position::new(Side::Long, 100.0, 0.0, "BTC/USD".to_string());
        assert!(matches!(result, Err(PositionError::InvalidQuantity(0.0))));
    }

    #[test]
    fn test_new_position_invalid_price() {
        let result = Position::new(Side::Long, 0.0, 1.0, "BTC/USD".to_string());
        assert!(matches!(result, Err(PositionError::InvalidEntryPrice(0.0))));
    }

    #[test]
    fn test_close_position() {
        let mut position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        position.close().unwrap();
        assert_eq!(position.status, Status::Closed);
    }

    #[test]
    fn test_close_already_closed() {
        let mut position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        position.close().unwrap();
        let result = position.close();
        assert!(matches!(result, Err(PositionError::AlreadyClosed)));
    }

    #[test]
    fn test_update_position() {
        let mut position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        position.update(2.0).unwrap();
        assert_eq!(position.quantity, 2.0);
    }

    #[test]
    fn test_update_closed_position() {
        let mut position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        position.close().unwrap();
        let result = position.update(2.0);
        assert!(matches!(result, Err(PositionError::AlreadyClosed)));
    }

    #[test]
    fn test_update_invalid_quantity() {
        let mut position = Position::new(Side::Long, 100.0, 1.0, "BTC/USD".to_string()).unwrap();
        let result = position.update(0.0);
        assert!(matches!(result, Err(PositionError::InvalidQuantity(0.0))));
    }
}