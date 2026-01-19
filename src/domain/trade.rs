use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a trade between two tokens
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub input_token: String,
    pub output_token: String,
    pub input_amount: f64,
    pub output_amount: f64,
    pub fees: Vec<Fee>,
}

/// Represents a fee associated with a trade
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fee {
    pub token: String,
    pub amount: f64,
}

/// Result of a trade execution attempt
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradeResult {
    Success {
        trade: Trade,
        timestamp: u64,
    },
    Failed {
        error: String,
        timestamp: u64,
    },
}

impl Trade {
    /// Creates a new trade with the given parameters
    pub fn new(
        input_token: String,
        output_token: String,
        input_amount: f64,
        output_amount: f64,
        fees: Vec<Fee>,
    ) -> Self {
        Self {
            input_token,
            output_token,
            input_amount,
            output_amount,
            fees,
        }
    }

    /// Simulates executing the trade with a given success probability
    pub fn simulate_execution(&self, success_probability: f64) -> TradeResult {
        let timestamp = chrono::Utc::now().timestamp() as u64;
        
        if rand::random::<f64>() <= success_probability {
            TradeResult::Success {
                trade: self.clone(),
                timestamp,
            }
        } else {
            TradeResult::Failed {
                error: "Trade execution failed randomly".to_string(),
                timestamp,
            }
        }
    }

    /// Calculates the total fees in terms of the input token
    pub fn total_fees_in_input(&self) -> f64 {
        self.fees
            .iter()
            .filter(|fee| fee.token == self.input_token)
            .map(|fee| fee.amount)
            .sum()
    }

    /// Calculates the total fees in terms of the output token
    pub fn total_fees_in_output(&self) -> f64 {
        self.fees
            .iter()
            .filter(|fee| fee.token == self.output_token)
            .map(|fee| fee.amount)
            .sum()
    }
}

impl fmt::Display for Trade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Trade: {} {} -> {} {}",
            self.input_amount, self.input_token, self.output_amount, self.output_token
        )
    }
}

impl fmt::Display for TradeResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeResult::Success { trade, timestamp } => {
                write!(f, "Success: {} at {}", trade, timestamp)
            }
            TradeResult::Failed { error, timestamp } => {
                write!(f, "Failed: {} at {}", error, timestamp)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_creation() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![Fee {
                token: "ETH".to_string(),
                amount: 0.001,
            }],
        );

        assert_eq!(trade.input_token, "ETH");
        assert_eq!(trade.output_token, "USDT");
        assert_eq!(trade.input_amount, 1.0);
        assert_eq!(trade.output_amount, 2000.0);
        assert_eq!(trade.fees.len(), 1);
    }

    #[test]
    fn test_fee_calculations() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![
                Fee {
                    token: "ETH".to_string(),
                    amount: 0.001,
                },
                Fee {
                    token: "USDT".to_string(),
                    amount: 2.0,
                },
            ],
        );

        assert_eq!(trade.total_fees_in_input(), 0.001);
        assert_eq!(trade.total_fees_in_output(), 2.0);
    }

    #[test]
    fn test_simulation_success() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![],
        );

        // With 100% probability should always succeed
        let result = trade.simulate_execution(1.0);
        assert!(matches!(result, TradeResult::Success { .. }));
    }

    #[test]
    fn test_simulation_failure() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![],
        );

        // With 0% probability should always fail
        let result = trade.simulate_execution(0.0);
        assert!(matches!(result, TradeResult::Failed { .. }));
    }

    #[test]
    fn test_display_trade() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![],
        );

        assert_eq!(
            format!("{}", trade),
            "Trade: 1 ETH -> 2000 USDT"
        );
    }

    #[test]
    fn test_display_trade_result() {
        let trade = Trade::new(
            "ETH".to_string(),
            "USDT".to_string(),
            1.0,
            2000.0,
            vec![],
        );

        let success = TradeResult::Success {
            trade: trade.clone(),
            timestamp: 1234567890,
        };
        assert!(format!("{}", success).contains("Success"));

        let failed = TradeResult::Failed {
            error: "test".to_string(),
            timestamp: 1234567890,
        };
        assert!(format!("{}", failed).contains("Failed"));
    }
}