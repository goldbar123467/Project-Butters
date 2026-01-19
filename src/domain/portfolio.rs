use std::collections::HashMap;

/// Represents a holding in the portfolio (simplified position for PnL tracking)
#[derive(Debug, Clone)]
pub struct Holding {
    pub symbol: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
}

impl Holding {
    pub fn new(symbol: String, quantity: f64, entry_price: f64) -> Self {
        Holding {
            symbol,
            quantity,
            entry_price,
            current_price: entry_price,
        }
    }

    pub fn update_price(&mut self, price: f64) {
        self.current_price = price;
    }

    pub fn pnl(&self) -> f64 {
        (self.current_price - self.entry_price) * self.quantity
    }
}

#[derive(Debug, Default)]
pub struct Portfolio {
    pub holdings: Vec<Holding>,
    pub balances: HashMap<String, f64>,
}

impl Portfolio {
    pub fn new() -> Self {
        Portfolio {
            holdings: Vec::new(),
            balances: HashMap::new(),
        }
    }

    pub fn add_holding(&mut self, holding: Holding) {
        self.holdings.push(holding);
    }

    pub fn remove_holding(&mut self, index: usize) -> Option<Holding> {
        if index < self.holdings.len() {
            Some(self.holdings.remove(index))
        } else {
            None
        }
    }

    pub fn update_holding_price(&mut self, symbol: &str, price: f64) -> bool {
        for holding in &mut self.holdings {
            if holding.symbol == symbol {
                holding.update_price(price);
                return true;
            }
        }
        false
    }

    pub fn total_pnl(&self) -> f64 {
        self.holdings.iter().map(|h| h.pnl()).sum()
    }

    pub fn update_balance(&mut self, asset: String, amount: f64) {
        *self.balances.entry(asset).or_insert(0.0) += amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_holding_pnl() {
        let mut holding = Holding::new("BTC".to_string(), 1.0, 50000.0);
        assert_eq!(holding.pnl(), 0.0);

        holding.update_price(55000.0);
        assert_eq!(holding.pnl(), 5000.0);

        holding.update_price(45000.0);
        assert_eq!(holding.pnl(), -5000.0);
    }

    #[test]
    fn test_portfolio_operations() {
        let mut portfolio = Portfolio::new();

        // Test adding holdings
        portfolio.add_holding(Holding::new("BTC".to_string(), 1.0, 50000.0));
        portfolio.add_holding(Holding::new("ETH".to_string(), 10.0, 3000.0));
        assert_eq!(portfolio.holdings.len(), 2);

        // Test updating prices
        portfolio.update_holding_price("BTC", 55000.0);
        portfolio.update_holding_price("ETH", 3500.0);
        assert_eq!(portfolio.total_pnl(), 10000.0); // 5000 + 5000

        // Test removing holdings
        let removed = portfolio.remove_holding(0);
        assert!(removed.is_some());
        assert_eq!(portfolio.holdings.len(), 1);

        // Test balance updates
        portfolio.update_balance("USD".to_string(), 1000.0);
        portfolio.update_balance("USD".to_string(), 500.0);
        assert_eq!(portfolio.balances.get("USD"), Some(&1500.0));
    }
}
