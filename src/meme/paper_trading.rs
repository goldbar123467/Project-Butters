//! Paper Trading Simulation Engine
//!
//! A simulation harness for testing meme coin trading strategies without real money.
//! Tracks simulated balances, token holdings, and calculates performance statistics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use tracing::{info, warn};

/// Token holding in the paper trading engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHolding {
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Token decimals
    pub decimals: u8,
    /// Amount of tokens held (in base units)
    pub amount: u64,
    /// Average cost basis per token in SOL
    pub avg_cost_sol: f64,
    /// Total SOL spent acquiring this position
    pub total_cost_sol: f64,
    /// Total realized PnL from sells (in SOL)
    pub realized_pnl_sol: f64,
    /// Number of buys
    pub buy_count: u32,
    /// Number of sells
    pub sell_count: u32,
}

impl TokenHolding {
    /// Create a new token holding
    pub fn new(mint: &str, symbol: &str, decimals: u8) -> Self {
        Self {
            mint: mint.to_string(),
            symbol: symbol.to_string(),
            decimals,
            amount: 0,
            avg_cost_sol: 0.0,
            total_cost_sol: 0.0,
            realized_pnl_sol: 0.0,
            buy_count: 0,
            sell_count: 0,
        }
    }

    /// Convert amount from base units to decimal representation
    pub fn amount_decimal(&self) -> f64 {
        self.amount as f64 / 10_u64.pow(self.decimals as u32) as f64
    }

    /// Calculate unrealized PnL given current price
    pub fn unrealized_pnl_sol(&self, current_price_sol: f64) -> f64 {
        let current_value = self.amount_decimal() * current_price_sol;
        current_value - self.total_cost_sol
    }

    /// Calculate total PnL (realized + unrealized)
    pub fn total_pnl_sol(&self, current_price_sol: f64) -> f64 {
        self.realized_pnl_sol + self.unrealized_pnl_sol(current_price_sol)
    }
}

/// A single paper trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTrade {
    /// Unique trade ID
    pub id: u64,
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Trade direction
    pub side: TradeSide,
    /// Token amount (in base units)
    pub token_amount: u64,
    /// Token decimals
    pub decimals: u8,
    /// SOL amount
    pub sol_amount: f64,
    /// Execution price (SOL per token)
    pub price: f64,
    /// Simulated slippage in basis points
    pub slippage_bps: u16,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
    /// PnL for this trade (sells only)
    pub pnl_sol: Option<f64>,
    /// PnL percentage (sells only)
    pub pnl_pct: Option<f64>,
}

impl PaperTrade {
    /// Convert token amount to decimal representation
    pub fn token_amount_decimal(&self) -> f64 {
        self.token_amount as f64 / 10_u64.pow(self.decimals as u32) as f64
    }
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    /// Buy tokens with SOL
    Buy,
    /// Sell tokens for SOL
    Sell,
}

impl std::fmt::Display for TradeSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeSide::Buy => write!(f, "BUY"),
            TradeSide::Sell => write!(f, "SELL"),
        }
    }
}

/// Paper trading statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaperStats {
    /// Total number of trades
    pub total_trades: u32,
    /// Number of buy trades
    pub buy_count: u32,
    /// Number of sell trades
    pub sell_count: u32,
    /// Number of winning trades (positive PnL)
    pub winning_trades: u32,
    /// Number of losing trades (negative PnL)
    pub losing_trades: u32,
    /// Total realized PnL in SOL
    pub total_realized_pnl_sol: f64,
    /// Total volume traded in SOL
    pub total_volume_sol: f64,
    /// Largest winning trade in SOL
    pub largest_win_sol: f64,
    /// Largest losing trade in SOL
    pub largest_loss_sol: f64,
    /// Sum of all wins (for average calculation)
    pub sum_wins_sol: f64,
    /// Sum of all losses (for average calculation)
    pub sum_losses_sol: f64,
    /// Peak portfolio value (for drawdown)
    pub peak_value_sol: f64,
    /// Maximum drawdown observed
    pub max_drawdown_pct: f64,
    /// Historical returns for Sharpe calculation
    trade_returns: Vec<f64>,
}

impl PaperStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Win rate as a percentage (0-100)
    pub fn win_rate(&self) -> f64 {
        let total_closed = self.winning_trades + self.losing_trades;
        if total_closed == 0 {
            return 0.0;
        }
        (self.winning_trades as f64 / total_closed as f64) * 100.0
    }

    /// Average winning trade in SOL
    pub fn avg_win_sol(&self) -> f64 {
        if self.winning_trades == 0 {
            return 0.0;
        }
        self.sum_wins_sol / self.winning_trades as f64
    }

    /// Average losing trade in SOL
    pub fn avg_loss_sol(&self) -> f64 {
        if self.losing_trades == 0 {
            return 0.0;
        }
        self.sum_losses_sol / self.losing_trades as f64
    }

    /// Profit factor (gross profits / gross losses)
    pub fn profit_factor(&self) -> f64 {
        if self.sum_losses_sol.abs() < 1e-10 {
            if self.sum_wins_sol > 0.0 {
                return f64::INFINITY;
            }
            return 0.0;
        }
        self.sum_wins_sol / self.sum_losses_sol.abs()
    }

    /// Sharpe ratio (annualized, assuming daily returns)
    /// Returns None if insufficient data (< 10 trades)
    pub fn sharpe_ratio(&self) -> Option<f64> {
        if self.trade_returns.len() < 10 {
            return None;
        }

        let n = self.trade_returns.len() as f64;
        let mean = self.trade_returns.iter().sum::<f64>() / n;

        let variance = self.trade_returns.iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / (n - 1.0);

        let std_dev = variance.sqrt();

        if std_dev < 1e-10 {
            return None;
        }

        // Annualize assuming ~252 trading days
        // Since meme trading is 24/7, use 365
        let annualization_factor = 365.0_f64.sqrt();
        Some((mean / std_dev) * annualization_factor)
    }

    /// Record a trade for statistics
    fn record_trade(&mut self, trade: &PaperTrade, portfolio_value: f64) {
        self.total_trades += 1;
        self.total_volume_sol += trade.sol_amount;

        match trade.side {
            TradeSide::Buy => {
                self.buy_count += 1;
            }
            TradeSide::Sell => {
                self.sell_count += 1;
                if let Some(pnl) = trade.pnl_sol {
                    self.total_realized_pnl_sol += pnl;

                    if pnl > 0.0 {
                        self.winning_trades += 1;
                        self.sum_wins_sol += pnl;
                        if pnl > self.largest_win_sol {
                            self.largest_win_sol = pnl;
                        }
                    } else if pnl < 0.0 {
                        self.losing_trades += 1;
                        self.sum_losses_sol += pnl.abs();
                        if pnl.abs() > self.largest_loss_sol {
                            self.largest_loss_sol = pnl.abs();
                        }
                    }

                    // Record return for Sharpe
                    if let Some(pnl_pct) = trade.pnl_pct {
                        self.trade_returns.push(pnl_pct / 100.0);
                    }
                }
            }
        }

        // Update peak and drawdown
        if portfolio_value > self.peak_value_sol {
            self.peak_value_sol = portfolio_value;
        } else if self.peak_value_sol > 0.0 {
            let drawdown = (self.peak_value_sol - portfolio_value) / self.peak_value_sol * 100.0;
            if drawdown > self.max_drawdown_pct {
                self.max_drawdown_pct = drawdown;
            }
        }
    }
}

/// Paper trading simulation engine
///
/// Simulates trading without real money, tracking balances, positions,
/// and performance statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTradingEngine {
    /// Simulated SOL balance
    sol_balance: f64,
    /// Initial SOL balance (for calculating returns)
    initial_sol_balance: f64,
    /// Token holdings by mint address
    token_holdings: HashMap<String, TokenHolding>,
    /// Trade history
    trades: Vec<PaperTrade>,
    /// Trading statistics
    stats: PaperStats,
    /// Next trade ID
    next_trade_id: u64,
    /// Simulated slippage in basis points (default 50 = 0.5%)
    slippage_bps: u16,
    /// Enable trade logging
    log_trades: bool,
}

impl PaperTradingEngine {
    /// Create a new paper trading engine with initial SOL balance
    ///
    /// # Arguments
    /// * `initial_sol` - Starting SOL balance for simulation
    pub fn new(initial_sol: f64) -> Self {
        info!(
            "Paper trading engine initialized with {} SOL",
            initial_sol
        );

        Self {
            sol_balance: initial_sol,
            initial_sol_balance: initial_sol,
            token_holdings: HashMap::new(),
            trades: Vec::new(),
            stats: PaperStats::new(),
            next_trade_id: 1,
            slippage_bps: 50,
            log_trades: true,
        }
    }

    /// Set the simulated slippage in basis points
    ///
    /// # Arguments
    /// * `slippage_bps` - Slippage in basis points (100 = 1%)
    pub fn set_slippage(&mut self, slippage_bps: u16) {
        self.slippage_bps = slippage_bps;
    }

    /// Enable or disable trade logging
    pub fn set_log_trades(&mut self, enabled: bool) {
        self.log_trades = enabled;
    }

    /// Get current SOL balance
    pub fn sol_balance(&self) -> f64 {
        self.sol_balance
    }

    /// Simulate buying tokens with SOL
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `symbol` - Token symbol
    /// * `decimals` - Token decimals
    /// * `sol_amount` - Amount of SOL to spend
    /// * `price` - Price per token in SOL (before slippage)
    ///
    /// # Returns
    /// Result containing the trade record or error message
    pub fn simulate_buy(
        &mut self,
        mint: &str,
        symbol: &str,
        decimals: u8,
        sol_amount: f64,
        price: f64,
    ) -> Result<PaperTrade, String> {
        // Validate inputs
        if sol_amount <= 0.0 {
            return Err("SOL amount must be positive".to_string());
        }
        if price <= 0.0 {
            return Err("Price must be positive".to_string());
        }
        if sol_amount > self.sol_balance {
            return Err(format!(
                "Insufficient SOL balance: have {:.4}, need {:.4}",
                self.sol_balance, sol_amount
            ));
        }

        // Apply slippage (buy = worse price = higher)
        let slippage_mult = 1.0 + (self.slippage_bps as f64 / 10000.0);
        let effective_price = price * slippage_mult;

        // Calculate token amount received
        let tokens_decimal = sol_amount / effective_price;
        let token_amount = (tokens_decimal * 10_u64.pow(decimals as u32) as f64) as u64;

        // Deduct SOL
        self.sol_balance -= sol_amount;

        // Update or create holding
        let holding = self.token_holdings
            .entry(mint.to_string())
            .or_insert_with(|| TokenHolding::new(mint, symbol, decimals));

        // Update average cost basis
        let prev_total = holding.amount_decimal() * holding.avg_cost_sol;
        let new_tokens_decimal = token_amount as f64 / 10_u64.pow(decimals as u32) as f64;
        let new_total = prev_total + sol_amount;

        holding.amount += token_amount;
        holding.total_cost_sol += sol_amount;

        if holding.amount > 0 {
            holding.avg_cost_sol = new_total / holding.amount_decimal();
        }
        holding.buy_count += 1;

        // Create trade record
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let trade = PaperTrade {
            id: self.next_trade_id,
            mint: mint.to_string(),
            symbol: symbol.to_string(),
            side: TradeSide::Buy,
            token_amount,
            decimals,
            sol_amount,
            price: effective_price,
            slippage_bps: self.slippage_bps,
            timestamp,
            pnl_sol: None,
            pnl_pct: None,
        };

        self.next_trade_id += 1;

        // Update stats
        let portfolio_value = self.calculate_portfolio_value_estimate();
        self.stats.record_trade(&trade, portfolio_value);

        // Log trade
        if self.log_trades {
            info!(
                "[PAPER] {} {} {} @ {:.8} SOL = {:.4} SOL (slippage: {}bps)",
                trade.side,
                new_tokens_decimal,
                symbol,
                effective_price,
                sol_amount,
                self.slippage_bps
            );
        }

        self.trades.push(trade.clone());
        Ok(trade)
    }

    /// Simulate selling tokens for SOL
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `token_amount` - Amount of tokens to sell (in base units)
    /// * `price` - Price per token in SOL (before slippage)
    ///
    /// # Returns
    /// Result containing the trade record or error message
    pub fn simulate_sell(
        &mut self,
        mint: &str,
        token_amount: u64,
        price: f64,
    ) -> Result<PaperTrade, String> {
        // Validate inputs
        if token_amount == 0 {
            return Err("Token amount must be positive".to_string());
        }
        if price <= 0.0 {
            return Err("Price must be positive".to_string());
        }

        // Check holding exists
        let holding = self.token_holdings
            .get_mut(mint)
            .ok_or_else(|| format!("No holding for token: {}", mint))?;

        if token_amount > holding.amount {
            return Err(format!(
                "Insufficient tokens: have {}, need {}",
                holding.amount, token_amount
            ));
        }

        // Apply slippage (sell = worse price = lower)
        let slippage_mult = 1.0 - (self.slippage_bps as f64 / 10000.0);
        let effective_price = price * slippage_mult;

        // Calculate SOL received
        let tokens_decimal = token_amount as f64 / 10_u64.pow(holding.decimals as u32) as f64;
        let sol_received = tokens_decimal * effective_price;

        // Calculate PnL
        let cost_basis = tokens_decimal * holding.avg_cost_sol;
        let pnl_sol = sol_received - cost_basis;
        let pnl_pct = if cost_basis > 0.0 {
            (pnl_sol / cost_basis) * 100.0
        } else {
            0.0
        };

        // Update holding
        let prev_amount = holding.amount;
        holding.amount -= token_amount;

        // Reduce total cost proportionally
        let fraction_sold = token_amount as f64 / prev_amount as f64;
        let cost_removed = holding.total_cost_sol * fraction_sold;
        holding.total_cost_sol -= cost_removed;

        holding.realized_pnl_sol += pnl_sol;
        holding.sell_count += 1;

        // Add SOL
        self.sol_balance += sol_received;

        // Store symbol and decimals before potentially removing holding
        let symbol = holding.symbol.clone();
        let decimals = holding.decimals;

        // Remove holding if fully sold
        if holding.amount == 0 {
            self.token_holdings.remove(mint);
        }

        // Create trade record
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let trade = PaperTrade {
            id: self.next_trade_id,
            mint: mint.to_string(),
            symbol: symbol.clone(),
            side: TradeSide::Sell,
            token_amount,
            decimals,
            sol_amount: sol_received,
            price: effective_price,
            slippage_bps: self.slippage_bps,
            timestamp,
            pnl_sol: Some(pnl_sol),
            pnl_pct: Some(pnl_pct),
        };

        self.next_trade_id += 1;

        // Update stats
        let portfolio_value = self.calculate_portfolio_value_estimate();
        self.stats.record_trade(&trade, portfolio_value);

        // Log trade
        if self.log_trades {
            let pnl_emoji = if pnl_sol >= 0.0 { "+" } else { "" };
            info!(
                "[PAPER] {} {} {} @ {:.8} SOL = {:.4} SOL | PnL: {}{:.4} SOL ({}{:.2}%)",
                trade.side,
                tokens_decimal,
                symbol,
                effective_price,
                sol_received,
                pnl_emoji,
                pnl_sol,
                pnl_emoji,
                pnl_pct
            );
        }

        self.trades.push(trade.clone());
        Ok(trade)
    }

    /// Get current position for a token
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    ///
    /// # Returns
    /// Option containing the token holding if exists
    pub fn get_position(&self, mint: &str) -> Option<&TokenHolding> {
        self.token_holdings.get(mint)
    }

    /// Get all current positions
    pub fn get_all_positions(&self) -> &HashMap<String, TokenHolding> {
        &self.token_holdings
    }

    /// Calculate total portfolio PnL (realized + unrealized at cost basis)
    pub fn get_pnl(&self) -> f64 {
        // Unrealized PnL (at cost basis, not current price)
        // This represents the difference between current holdings cost and initial investment
        let total_invested: f64 = self.token_holdings.values()
            .map(|h| h.total_cost_sol)
            .sum();

        // Total PnL = current SOL balance + holdings cost - initial balance
        self.sol_balance + total_invested - self.initial_sol_balance
    }

    /// Calculate total PnL with current market prices
    ///
    /// # Arguments
    /// * `prices` - Map of mint address to current price in SOL
    pub fn get_pnl_with_prices(&self, prices: &HashMap<String, f64>) -> f64 {
        let portfolio_value = self.calculate_portfolio_value(prices);
        portfolio_value - self.initial_sol_balance
    }

    /// Get trading statistics
    pub fn get_stats(&self) -> &PaperStats {
        &self.stats
    }

    /// Get all trade history
    pub fn get_trades(&self) -> &[PaperTrade] {
        &self.trades
    }

    /// Get trades for a specific token
    pub fn get_trades_for_token(&self, mint: &str) -> Vec<&PaperTrade> {
        self.trades.iter()
            .filter(|t| t.mint == mint)
            .collect()
    }

    /// Calculate current portfolio value with market prices
    ///
    /// # Arguments
    /// * `prices` - Map of mint address to current price in SOL
    pub fn calculate_portfolio_value(&self, prices: &HashMap<String, f64>) -> f64 {
        let mut total = self.sol_balance;

        for (mint, holding) in &self.token_holdings {
            if let Some(&price) = prices.get(mint) {
                total += holding.amount_decimal() * price;
            } else {
                // Fall back to cost basis if no price available
                warn!(
                    "No price available for {}, using cost basis",
                    holding.symbol
                );
                total += holding.total_cost_sol;
            }
        }

        total
    }

    /// Estimate portfolio value using cost basis (when prices not available)
    fn calculate_portfolio_value_estimate(&self) -> f64 {
        let holdings_value: f64 = self.token_holdings.values()
            .map(|h| h.total_cost_sol)
            .sum();
        self.sol_balance + holdings_value
    }

    /// Get return percentage since inception
    pub fn get_return_pct(&self) -> f64 {
        if self.initial_sol_balance <= 0.0 {
            return 0.0;
        }
        let current = self.calculate_portfolio_value_estimate();
        ((current - self.initial_sol_balance) / self.initial_sol_balance) * 100.0
    }

    /// Get return percentage with market prices
    pub fn get_return_pct_with_prices(&self, prices: &HashMap<String, f64>) -> f64 {
        if self.initial_sol_balance <= 0.0 {
            return 0.0;
        }
        let current = self.calculate_portfolio_value(prices);
        ((current - self.initial_sol_balance) / self.initial_sol_balance) * 100.0
    }

    /// Reset the paper trading engine
    pub fn reset(&mut self) {
        info!(
            "Paper trading engine reset with {} SOL",
            self.initial_sol_balance
        );

        self.sol_balance = self.initial_sol_balance;
        self.token_holdings.clear();
        self.trades.clear();
        self.stats = PaperStats::new();
        self.next_trade_id = 1;
    }

    /// Export trade history to JSON
    pub fn export_trades_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.trades)
            .map_err(|e| format!("Failed to serialize trades: {}", e))
    }

    /// Export full state to JSON
    pub fn export_state_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize state: {}", e))
    }

    /// Import state from JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json)
            .map_err(|e| format!("Failed to deserialize state: {}", e))
    }

    /// Print a summary of current state
    pub fn print_summary(&self) {
        info!("========== PAPER TRADING SUMMARY ==========");
        info!("SOL Balance: {:.4}", self.sol_balance);
        info!("Initial Balance: {:.4}", self.initial_sol_balance);
        info!("Return: {:.2}%", self.get_return_pct());
        info!("");
        info!("Active Positions: {}", self.token_holdings.len());
        for holding in self.token_holdings.values() {
            info!(
                "  {} {}: {} (cost basis: {:.4} SOL)",
                holding.symbol,
                &holding.mint[..8],
                holding.amount_decimal(),
                holding.total_cost_sol
            );
        }
        info!("");
        info!("Statistics:");
        info!("  Total Trades: {}", self.stats.total_trades);
        info!("  Win Rate: {:.1}%", self.stats.win_rate());
        info!("  Avg Win: {:.4} SOL", self.stats.avg_win_sol());
        info!("  Avg Loss: {:.4} SOL", self.stats.avg_loss_sol());
        info!("  Profit Factor: {:.2}", self.stats.profit_factor());
        info!("  Max Drawdown: {:.2}%", self.stats.max_drawdown_pct);
        if let Some(sharpe) = self.stats.sharpe_ratio() {
            info!("  Sharpe Ratio: {:.2}", sharpe);
        }
        info!("==========================================");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_engine() {
        let engine = PaperTradingEngine::new(10.0);
        assert_eq!(engine.sol_balance(), 10.0);
        assert!(engine.get_all_positions().is_empty());
        assert!(engine.get_trades().is_empty());
    }

    #[test]
    fn test_simulate_buy() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        // Buy tokens at 0.001 SOL each with 1 SOL
        let result = engine.simulate_buy(
            "test_mint",
            "TEST",
            9,  // 9 decimals
            1.0,
            0.001,
        );

        assert!(result.is_ok());
        let trade = result.unwrap();

        assert_eq!(trade.side, TradeSide::Buy);
        assert_eq!(trade.sol_amount, 1.0);

        // Check balance reduced
        assert!(engine.sol_balance() < 10.0);

        // Check holding created
        let holding = engine.get_position("test_mint").unwrap();
        assert!(holding.amount > 0);
    }

    #[test]
    fn test_simulate_buy_insufficient_balance() {
        let mut engine = PaperTradingEngine::new(1.0);
        engine.set_log_trades(false);

        let result = engine.simulate_buy(
            "test_mint",
            "TEST",
            9,
            2.0,  // More than balance
            0.001,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient SOL"));
    }

    #[test]
    fn test_simulate_sell() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        // First buy
        let buy = engine.simulate_buy(
            "test_mint",
            "TEST",
            9,
            1.0,
            0.001,
        ).unwrap();

        let tokens_bought = buy.token_amount;

        // Sell half at higher price (profit)
        let sell = engine.simulate_sell(
            "test_mint",
            tokens_bought / 2,
            0.002,  // 2x price
        ).unwrap();

        assert_eq!(sell.side, TradeSide::Sell);
        assert!(sell.pnl_sol.is_some());
        assert!(sell.pnl_sol.unwrap() > 0.0);  // Should be profitable
    }

    #[test]
    fn test_simulate_sell_insufficient_tokens() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        engine.simulate_buy(
            "test_mint",
            "TEST",
            9,
            1.0,
            0.001,
        ).unwrap();

        let result = engine.simulate_sell(
            "test_mint",
            u64::MAX,  // Way more than we have
            0.001,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient tokens"));
    }

    #[test]
    fn test_simulate_sell_no_holding() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        let result = engine.simulate_sell(
            "nonexistent_mint",
            1000,
            0.001,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No holding"));
    }

    #[test]
    fn test_slippage_application() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);
        engine.set_slippage(100);  // 1% slippage

        // Buy - price should be higher
        let buy = engine.simulate_buy(
            "test_mint",
            "TEST",
            9,
            1.0,
            0.001,
        ).unwrap();

        assert!(buy.price > 0.001);  // Price with slippage

        // Setup for sell
        let mut engine2 = PaperTradingEngine::new(10.0);
        engine2.set_log_trades(false);
        engine2.set_slippage(100);

        engine2.simulate_buy("test_mint", "TEST", 9, 1.0, 0.001).unwrap();

        let holding = engine2.get_position("test_mint").unwrap();
        let sell = engine2.simulate_sell(
            "test_mint",
            holding.amount,
            0.001,
        ).unwrap();

        assert!(sell.price < 0.001);  // Price with slippage (worse for sell)
    }

    #[test]
    fn test_stats_tracking() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);
        engine.set_slippage(0);  // No slippage for easier calculation

        // Win: buy low, sell high
        engine.simulate_buy("test1", "TEST1", 9, 1.0, 0.001).unwrap();
        let holding = engine.get_position("test1").unwrap().amount;
        engine.simulate_sell("test1", holding, 0.002).unwrap();  // 2x price

        // Loss: buy high, sell low
        engine.simulate_buy("test2", "TEST2", 9, 1.0, 0.002).unwrap();
        let holding = engine.get_position("test2").unwrap().amount;
        engine.simulate_sell("test2", holding, 0.001).unwrap();  // 0.5x price

        let stats = engine.get_stats();
        assert_eq!(stats.total_trades, 4);
        assert_eq!(stats.buy_count, 2);
        assert_eq!(stats.sell_count, 2);
        assert_eq!(stats.winning_trades, 1);
        assert_eq!(stats.losing_trades, 1);
        assert_eq!(stats.win_rate(), 50.0);
    }

    #[test]
    fn test_pnl_calculation() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);
        engine.set_slippage(0);

        // Buy and sell at same price = 0 PnL
        engine.simulate_buy("test", "TEST", 9, 1.0, 0.001).unwrap();
        let holding = engine.get_position("test").unwrap().amount;
        engine.simulate_sell("test", holding, 0.001).unwrap();

        // Should be back to 10 SOL (minus any rounding)
        assert!((engine.sol_balance() - 10.0).abs() < 0.0001);
    }

    #[test]
    fn test_portfolio_value() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);
        engine.set_slippage(0);

        // Buy 1 SOL worth of tokens
        engine.simulate_buy("test", "TEST", 9, 1.0, 0.001).unwrap();

        // Portfolio value with same price = 10 SOL
        let mut prices = HashMap::new();
        prices.insert("test".to_string(), 0.001);

        let value = engine.calculate_portfolio_value(&prices);
        assert!((value - 10.0).abs() < 0.0001);

        // Portfolio value with 2x price
        prices.insert("test".to_string(), 0.002);
        let value = engine.calculate_portfolio_value(&prices);
        assert!(value > 10.0);  // Should be ~11 SOL
    }

    #[test]
    fn test_reset() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        engine.simulate_buy("test", "TEST", 9, 1.0, 0.001).unwrap();

        assert!(!engine.get_all_positions().is_empty());
        assert!(!engine.get_trades().is_empty());

        engine.reset();

        assert!(engine.get_all_positions().is_empty());
        assert!(engine.get_trades().is_empty());
        assert_eq!(engine.sol_balance(), 10.0);
    }

    #[test]
    fn test_token_holding_calculations() {
        let mut holding = TokenHolding::new("test", "TEST", 9);
        holding.amount = 1_000_000_000;  // 1 token with 9 decimals
        holding.avg_cost_sol = 0.5;
        holding.total_cost_sol = 0.5;

        assert_eq!(holding.amount_decimal(), 1.0);

        // Unrealized PnL at 2x price
        let pnl = holding.unrealized_pnl_sol(1.0);
        assert!((pnl - 0.5).abs() < 0.0001);  // 1.0 - 0.5 = 0.5 profit
    }

    #[test]
    fn test_paper_stats_win_rate() {
        let mut stats = PaperStats::new();

        assert_eq!(stats.win_rate(), 0.0);  // No trades

        stats.winning_trades = 3;
        stats.losing_trades = 1;

        assert_eq!(stats.win_rate(), 75.0);
    }

    #[test]
    fn test_paper_stats_profit_factor() {
        let mut stats = PaperStats::new();

        stats.sum_wins_sol = 2.0;
        stats.sum_losses_sol = 1.0;

        assert_eq!(stats.profit_factor(), 2.0);
    }

    #[test]
    fn test_export_import_state() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        engine.simulate_buy("test", "TEST", 9, 1.0, 0.001).unwrap();

        let json = engine.export_state_json().unwrap();
        let restored = PaperTradingEngine::from_json(&json).unwrap();

        assert_eq!(restored.sol_balance(), engine.sol_balance());
        assert_eq!(restored.get_trades().len(), engine.get_trades().len());
    }

    #[test]
    fn test_multiple_buys_avg_cost() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);
        engine.set_slippage(0);

        // Buy 1 SOL at 0.001
        engine.simulate_buy("test", "TEST", 9, 1.0, 0.001).unwrap();

        // Buy 1 SOL at 0.002
        engine.simulate_buy("test", "TEST", 9, 1.0, 0.002).unwrap();

        let holding = engine.get_position("test").unwrap();

        // Total cost = 2 SOL
        assert!((holding.total_cost_sol - 2.0).abs() < 0.0001);

        // Average cost should be between 0.001 and 0.002
        assert!(holding.avg_cost_sol > 0.001 && holding.avg_cost_sol < 0.002);
    }

    #[test]
    fn test_get_trades_for_token() {
        let mut engine = PaperTradingEngine::new(10.0);
        engine.set_log_trades(false);

        engine.simulate_buy("test1", "TEST1", 9, 1.0, 0.001).unwrap();
        engine.simulate_buy("test2", "TEST2", 9, 1.0, 0.001).unwrap();
        engine.simulate_buy("test1", "TEST1", 9, 1.0, 0.001).unwrap();

        let test1_trades = engine.get_trades_for_token("test1");
        assert_eq!(test1_trades.len(), 2);

        let test2_trades = engine.get_trades_for_token("test2");
        assert_eq!(test2_trades.len(), 1);
    }
}
