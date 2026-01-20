//! Pump.fun Types
//!
//! Data types for pump.fun WebSocket messages and token information.

use serde::{Deserialize, Serialize};

/// Token information from pump.fun
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunToken {
    /// Token mint address
    pub mint: String,
    /// Token name
    pub name: String,
    /// Token symbol
    pub symbol: String,
    /// Token description (from metadata URI)
    #[serde(default)]
    pub description: Option<String>,
    /// Metadata URI (usually IPFS)
    #[serde(default)]
    pub uri: Option<String>,
    /// Creator wallet address
    #[serde(rename = "traderPublicKey")]
    pub creator: String,
    /// Initial buy amount in token units
    #[serde(rename = "initialBuy", default)]
    pub initial_buy: u64,
    /// Initial market cap in SOL
    #[serde(rename = "marketCapSol", default)]
    pub market_cap_sol: f64,
    /// Token image URL (from metadata)
    #[serde(default)]
    pub image_url: Option<String>,
    /// Twitter handle (from metadata)
    #[serde(default)]
    pub twitter: Option<String>,
    /// Telegram link (from metadata)
    #[serde(default)]
    pub telegram: Option<String>,
    /// Website URL (from metadata)
    #[serde(default)]
    pub website: Option<String>,
    /// Unix timestamp of creation
    #[serde(default)]
    pub timestamp: u64,
}

impl PumpFunToken {
    /// Create a new PumpFunToken with minimal fields
    pub fn new(mint: String, name: String, symbol: String, creator: String) -> Self {
        Self {
            mint,
            name,
            symbol,
            description: None,
            uri: None,
            creator,
            initial_buy: 0,
            market_cap_sol: 0.0,
            image_url: None,
            twitter: None,
            telegram: None,
            website: None,
            timestamp: 0,
        }
    }

    /// Check if token has social links (potentially more legitimate)
    pub fn has_socials(&self) -> bool {
        self.twitter.is_some() || self.telegram.is_some() || self.website.is_some()
    }

    /// Check if token has metadata
    pub fn has_metadata(&self) -> bool {
        self.uri.is_some() || self.description.is_some()
    }
}

/// Bonding curve state for a pump.fun token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondingCurveState {
    /// Token mint address
    pub mint: String,
    /// Virtual SOL reserves
    #[serde(rename = "virtualSolReserves")]
    pub virtual_sol_reserves: u64,
    /// Virtual token reserves
    #[serde(rename = "virtualTokenReserves")]
    pub virtual_token_reserves: u64,
    /// Real SOL reserves (actual SOL in curve)
    #[serde(rename = "realSolReserves", default)]
    pub real_sol_reserves: u64,
    /// Real token reserves (actual tokens in curve)
    #[serde(rename = "realTokenReserves", default)]
    pub real_token_reserves: u64,
    /// Token total supply
    #[serde(rename = "tokenTotalSupply", default)]
    pub token_total_supply: u64,
    /// Whether the bonding curve has completed (token graduated)
    #[serde(default)]
    pub complete: bool,
}

impl BondingCurveState {
    /// Calculate current market cap in SOL
    pub fn market_cap_sol(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }
        // Price = virtual_sol / virtual_tokens
        // Market cap = price * total_supply
        let price = self.virtual_sol_reserves as f64 / self.virtual_token_reserves as f64;
        let supply = if self.token_total_supply > 0 {
            self.token_total_supply as f64
        } else {
            1_000_000_000_000_000.0 // Default 1B supply with 6 decimals
        };
        price * supply / 1e9 // Convert lamports to SOL
    }

    /// Calculate bonding curve completion percentage
    /// Pump.fun tokens graduate at ~$69k market cap (~85 SOL)
    pub fn graduation_progress(&self) -> f64 {
        const GRADUATION_SOL: f64 = 85.0;
        let progress = (self.real_sol_reserves as f64 / 1e9) / GRADUATION_SOL * 100.0;
        progress.min(100.0)
    }

    /// Check if token is close to graduation (>80%)
    pub fn is_near_graduation(&self) -> bool {
        self.graduation_progress() >= 80.0
    }

    /// Calculate price in SOL per token (with 6 decimals)
    pub fn price_per_token(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }
        (self.virtual_sol_reserves as f64) / (self.virtual_token_reserves as f64) / 1e3
    }
}

/// Trade information from pump.fun WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeInfo {
    /// Token mint address
    pub mint: String,
    /// Signature of the transaction
    #[serde(default)]
    pub signature: Option<String>,
    /// Trader wallet address
    #[serde(rename = "traderPublicKey")]
    pub trader: String,
    /// Whether this is a buy (true) or sell (false)
    #[serde(rename = "txType", default)]
    pub is_buy: bool,
    /// SOL amount in lamports
    #[serde(rename = "solAmount", default)]
    pub sol_amount: u64,
    /// Token amount (with decimals)
    #[serde(rename = "tokenAmount", default)]
    pub token_amount: u64,
    /// New market cap in SOL after trade
    #[serde(rename = "marketCapSol", default)]
    pub market_cap_sol: f64,
    /// New virtual SOL reserves
    #[serde(rename = "vSolInBondingCurve", default)]
    pub virtual_sol_reserves: u64,
    /// New virtual token reserves
    #[serde(rename = "vTokensInBondingCurve", default)]
    pub virtual_token_reserves: u64,
    /// Unix timestamp
    #[serde(default)]
    pub timestamp: u64,
}

impl TradeInfo {
    /// Get SOL amount in SOL (not lamports)
    pub fn sol_amount_f64(&self) -> f64 {
        self.sol_amount as f64 / 1e9
    }

    /// Get token amount as f64 (accounting for 6 decimals)
    pub fn token_amount_f64(&self) -> f64 {
        self.token_amount as f64 / 1e6
    }

    /// Calculate effective price per token
    pub fn price_per_token(&self) -> f64 {
        if self.token_amount == 0 {
            return 0.0;
        }
        self.sol_amount_f64() / self.token_amount_f64()
    }

    /// Check if this is a significant trade (> 0.1 SOL)
    pub fn is_significant(&self) -> bool {
        self.sol_amount_f64() >= 0.1
    }

    /// Check if this is a whale trade (> 1 SOL)
    pub fn is_whale_trade(&self) -> bool {
        self.sol_amount_f64() >= 1.0
    }
}

/// WebSocket subscription message
#[derive(Debug, Clone, Serialize)]
pub struct SubscribeMessage {
    /// Method to call
    pub method: String,
    /// Optional keys (e.g., mint addresses or account addresses)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
}

impl SubscribeMessage {
    /// Subscribe to new token launches
    pub fn new_token() -> Self {
        Self {
            method: "subscribeNewToken".to_string(),
            keys: None,
        }
    }

    /// Subscribe to trades on specific tokens
    pub fn token_trades(mints: Vec<String>) -> Self {
        Self {
            method: "subscribeTokenTrade".to_string(),
            keys: Some(mints),
        }
    }

    /// Subscribe to trades by specific accounts
    pub fn account_trades(accounts: Vec<String>) -> Self {
        Self {
            method: "subscribeAccountTrade".to_string(),
            keys: Some(accounts),
        }
    }

    /// Unsubscribe from new token launches
    pub fn unsubscribe_new_token() -> Self {
        Self {
            method: "unsubscribeNewToken".to_string(),
            keys: None,
        }
    }

    /// Unsubscribe from token trades
    pub fn unsubscribe_token_trades(mints: Vec<String>) -> Self {
        Self {
            method: "unsubscribeTokenTrade".to_string(),
            keys: Some(mints),
        }
    }
}

/// Raw WebSocket message from pump.fun
/// Used for initial parsing before determining message type
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RawPumpMessage {
    /// New token creation event
    NewToken(PumpFunToken),
    /// Trade event
    Trade(TradeInfo),
    /// Subscription confirmation
    Confirmation { message: String },
    /// Error message
    Error { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pump_fun_token_new() {
        let token = PumpFunToken::new(
            "mint123".to_string(),
            "Test Token".to_string(),
            "TEST".to_string(),
            "creator456".to_string(),
        );

        assert_eq!(token.mint, "mint123");
        assert_eq!(token.name, "Test Token");
        assert_eq!(token.symbol, "TEST");
        assert_eq!(token.creator, "creator456");
        assert!(!token.has_socials());
        assert!(!token.has_metadata());
    }

    #[test]
    fn test_pump_fun_token_with_socials() {
        let mut token = PumpFunToken::new(
            "mint123".to_string(),
            "Test Token".to_string(),
            "TEST".to_string(),
            "creator456".to_string(),
        );
        token.twitter = Some("@test".to_string());

        assert!(token.has_socials());
    }

    #[test]
    fn test_bonding_curve_graduation_progress() {
        let curve = BondingCurveState {
            mint: "mint123".to_string(),
            virtual_sol_reserves: 30_000_000_000, // 30 SOL in lamports
            virtual_token_reserves: 1_000_000_000_000_000,
            real_sol_reserves: 42_500_000_000, // 42.5 SOL = 50% of 85 SOL
            real_token_reserves: 500_000_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        let progress = curve.graduation_progress();
        assert!((progress - 50.0).abs() < 0.1);
        assert!(!curve.is_near_graduation());
    }

    #[test]
    fn test_bonding_curve_near_graduation() {
        let curve = BondingCurveState {
            mint: "mint123".to_string(),
            virtual_sol_reserves: 30_000_000_000,
            virtual_token_reserves: 1_000_000_000_000_000,
            real_sol_reserves: 70_000_000_000, // 70 SOL = ~82% of 85 SOL
            real_token_reserves: 200_000_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        assert!(curve.is_near_graduation());
    }

    #[test]
    fn test_trade_info_amounts() {
        let trade = TradeInfo {
            mint: "mint123".to_string(),
            signature: Some("sig123".to_string()),
            trader: "trader456".to_string(),
            is_buy: true,
            sol_amount: 1_500_000_000, // 1.5 SOL
            token_amount: 1_000_000_000, // 1000 tokens (6 decimals)
            market_cap_sol: 50.0,
            virtual_sol_reserves: 0,
            virtual_token_reserves: 0,
            timestamp: 0,
        };

        assert!((trade.sol_amount_f64() - 1.5).abs() < 0.001);
        assert!((trade.token_amount_f64() - 1000.0).abs() < 0.001);
        assert!(trade.is_significant());
        assert!(trade.is_whale_trade());
    }

    #[test]
    fn test_trade_info_small_trade() {
        let trade = TradeInfo {
            mint: "mint123".to_string(),
            signature: None,
            trader: "trader456".to_string(),
            is_buy: true,
            sol_amount: 50_000_000, // 0.05 SOL
            token_amount: 100_000_000,
            market_cap_sol: 30.0,
            virtual_sol_reserves: 0,
            virtual_token_reserves: 0,
            timestamp: 0,
        };

        assert!(!trade.is_significant());
        assert!(!trade.is_whale_trade());
    }

    #[test]
    fn test_subscribe_message_new_token() {
        let msg = SubscribeMessage::new_token();
        assert_eq!(msg.method, "subscribeNewToken");
        assert!(msg.keys.is_none());

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribeNewToken"));
        assert!(!json.contains("keys"));
    }

    #[test]
    fn test_subscribe_message_token_trades() {
        let msg = SubscribeMessage::token_trades(vec!["mint1".to_string(), "mint2".to_string()]);
        assert_eq!(msg.method, "subscribeTokenTrade");
        assert_eq!(msg.keys, Some(vec!["mint1".to_string(), "mint2".to_string()]));

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribeTokenTrade"));
        assert!(json.contains("mint1"));
    }

    #[test]
    fn test_subscribe_message_account_trades() {
        let msg = SubscribeMessage::account_trades(vec!["account1".to_string()]);
        assert_eq!(msg.method, "subscribeAccountTrade");
    }

    #[test]
    fn test_parse_new_token_message() {
        let json = r#"{
            "mint": "TokenMint123",
            "name": "Test Meme",
            "symbol": "MEME",
            "uri": "https://ipfs.io/ipfs/abc",
            "traderPublicKey": "Creator456",
            "initialBuy": 1000000,
            "marketCapSol": 30.5
        }"#;

        let token: PumpFunToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.mint, "TokenMint123");
        assert_eq!(token.name, "Test Meme");
        assert_eq!(token.symbol, "MEME");
        assert_eq!(token.creator, "Creator456");
        assert_eq!(token.initial_buy, 1000000);
        assert!((token.market_cap_sol - 30.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_trade_message() {
        let json = r#"{
            "mint": "TokenMint123",
            "traderPublicKey": "Trader789",
            "txType": true,
            "solAmount": 2000000000,
            "tokenAmount": 500000000,
            "marketCapSol": 35.0,
            "vSolInBondingCurve": 32000000000,
            "vTokensInBondingCurve": 950000000000000
        }"#;

        let trade: TradeInfo = serde_json::from_str(json).unwrap();
        assert_eq!(trade.mint, "TokenMint123");
        assert!(trade.is_buy);
        assert_eq!(trade.sol_amount, 2000000000);
    }

    #[test]
    fn test_bonding_curve_price() {
        let curve = BondingCurveState {
            mint: "mint123".to_string(),
            virtual_sol_reserves: 30_000_000_000, // 30 SOL
            virtual_token_reserves: 1_000_000_000_000_000, // 1B tokens
            real_sol_reserves: 0,
            real_token_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        let price = curve.price_per_token();
        // 30 SOL / 1B tokens * 1000 (to adjust for decimal calc) = 0.00003 SOL per token
        assert!(price > 0.0);
    }
}
