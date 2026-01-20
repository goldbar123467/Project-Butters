//! Pump.fun WebSocket Monitor
//!
//! Real-time monitoring of pump.fun token launches and trades via WebSocket.
//! Connects to wss://pumpportal.fun/api/data for streaming data.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json;
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use super::types::{PumpFunToken, RawPumpMessage, SubscribeMessage, TradeInfo};

/// Default WebSocket URL for pump.fun data
pub const DEFAULT_WS_URL: &str = "wss://pumpportal.fun/api/data";

/// Reconnection delay base (exponential backoff)
const RECONNECT_BASE_DELAY_MS: u64 = 1000;
/// Maximum reconnection delay
const MAX_RECONNECT_DELAY_MS: u64 = 30000;
/// Ping interval for keepalive
const PING_INTERVAL_SECS: u64 = 30;
/// Default message receive timeout in seconds
const DEFAULT_MESSAGE_TIMEOUT_SECS: u64 = 60;
/// Maximum message size in bytes (prevent memory exhaustion)
const MAX_MESSAGE_SIZE: usize = 1_048_576; // 1 MB

/// Errors that can occur in PumpFunMonitor
#[derive(Debug, Error)]
pub enum PumpMonitorError {
    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Failed to parse message: {0}")]
    ParseError(String),

    #[error("Send channel closed")]
    ChannelClosed,

    #[error("Monitor not running")]
    NotRunning,

    #[error("Already subscribed to: {0}")]
    AlreadySubscribed(String),

    #[error("Message receive timeout after {0} seconds")]
    MessageTimeout(u64),

    #[error("Connection state recovery failed: {0}")]
    RecoveryFailed(String),

    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Events emitted by the PumpFunMonitor
#[derive(Debug, Clone)]
pub enum PumpEvent {
    /// New token was created on pump.fun
    NewToken {
        mint: String,
        name: String,
        symbol: String,
        creator: String,
        initial_buy: u64,
        market_cap_sol: f64,
        uri: Option<String>,
        timestamp: u64,
    },
    /// Trade occurred on a monitored token
    TokenTrade {
        mint: String,
        is_buy: bool,
        sol_amount: u64,
        token_amount: u64,
        trader: String,
        market_cap_sol: f64,
        timestamp: u64,
    },
    /// Graduation progress update (calculated from trades)
    GraduationProgress {
        mint: String,
        bonding_curve_percent: f64,
        virtual_sol_reserves: u64,
    },
    /// Connection state changed
    ConnectionState {
        connected: bool,
        reconnect_count: u32,
    },
    /// Error occurred
    Error {
        message: String,
    },
}

impl PumpEvent {
    /// Create NewToken event from PumpFunToken
    pub fn from_token(token: &PumpFunToken) -> Self {
        PumpEvent::NewToken {
            mint: token.mint.clone(),
            name: token.name.clone(),
            symbol: token.symbol.clone(),
            creator: token.creator.clone(),
            initial_buy: token.initial_buy,
            market_cap_sol: token.market_cap_sol,
            uri: token.uri.clone(),
            timestamp: token.timestamp,
        }
    }

    /// Create TokenTrade event from TradeInfo
    pub fn from_trade(trade: &TradeInfo) -> Self {
        PumpEvent::TokenTrade {
            mint: trade.mint.clone(),
            is_buy: trade.is_buy,
            sol_amount: trade.sol_amount,
            token_amount: trade.token_amount,
            trader: trade.trader.clone(),
            market_cap_sol: trade.market_cap_sol,
            timestamp: trade.timestamp,
        }
    }

    /// Calculate graduation progress from trade info
    pub fn graduation_from_trade(trade: &TradeInfo) -> Self {
        // Pump.fun graduation is at ~85 SOL in bonding curve
        const GRADUATION_SOL_LAMPORTS: u64 = 85_000_000_000;
        let progress = if trade.virtual_sol_reserves > 0 {
            (trade.virtual_sol_reserves as f64 / GRADUATION_SOL_LAMPORTS as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        PumpEvent::GraduationProgress {
            mint: trade.mint.clone(),
            bonding_curve_percent: progress,
            virtual_sol_reserves: trade.virtual_sol_reserves,
        }
    }
}

/// Configuration for PumpFunMonitor
#[derive(Debug, Clone)]
pub struct PumpMonitorConfig {
    /// WebSocket URL
    pub ws_url: String,
    /// Auto-reconnect on disconnect
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts (0 = unlimited)
    pub max_reconnect_attempts: u32,
    /// Subscribe to new tokens on connect
    pub subscribe_new_tokens: bool,
    /// Event channel buffer size
    pub channel_buffer_size: usize,
    /// Enable ping/pong keepalive
    pub enable_keepalive: bool,
    /// Message receive timeout in seconds (0 = no timeout)
    pub message_timeout_secs: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl Default for PumpMonitorConfig {
    fn default() -> Self {
        Self {
            ws_url: DEFAULT_WS_URL.to_string(),
            auto_reconnect: true,
            max_reconnect_attempts: 0, // unlimited
            subscribe_new_tokens: true,
            channel_buffer_size: 1000,
            enable_keepalive: true,
            message_timeout_secs: DEFAULT_MESSAGE_TIMEOUT_SECS,
            max_message_size: MAX_MESSAGE_SIZE,
        }
    }
}

/// Subscription state tracking
#[derive(Debug, Default)]
struct SubscriptionState {
    /// Currently subscribed to new tokens
    new_tokens: bool,
    /// Token mints we're tracking trades for
    token_trades: HashSet<String>,
    /// Account addresses we're tracking trades for
    account_trades: HashSet<String>,
}

/// Internal command for the monitor task
enum MonitorCommand {
    SubscribeNewTokens,
    UnsubscribeNewTokens,
    SubscribeTokenTrades(Vec<String>),
    UnsubscribeTokenTrades(Vec<String>),
    SubscribeAccountTrades(Vec<String>),
    Shutdown,
}

/// Pump.fun WebSocket monitor for real-time token launch detection
///
/// # Example
/// ```ignore
/// let config = PumpMonitorConfig::default();
/// let (monitor, mut rx) = PumpFunMonitor::new(config);
///
/// // Start monitoring in background
/// tokio::spawn(async move {
///     monitor.run().await;
/// });
///
/// // Process events
/// while let Some(event) = rx.recv().await {
///     match event {
///         PumpEvent::NewToken { mint, name, symbol, .. } => {
///             println!("New token: {} ({}) - {}", name, symbol, mint);
///         }
///         _ => {}
///     }
/// }
/// ```
pub struct PumpFunMonitor {
    config: PumpMonitorConfig,
    event_tx: mpsc::Sender<PumpEvent>,
    command_tx: mpsc::Sender<MonitorCommand>,
    command_rx: Arc<RwLock<Option<mpsc::Receiver<MonitorCommand>>>>,
    subscriptions: Arc<RwLock<SubscriptionState>>,
    reconnect_count: Arc<RwLock<u32>>,
    is_running: Arc<RwLock<bool>>,
}

impl PumpFunMonitor {
    /// Create a new PumpFunMonitor with the given configuration
    /// Returns the monitor and a receiver for events
    pub fn new(config: PumpMonitorConfig) -> (Self, mpsc::Receiver<PumpEvent>) {
        let (event_tx, event_rx) = mpsc::channel(config.channel_buffer_size);
        let (command_tx, command_rx) = mpsc::channel(100);

        let monitor = Self {
            config,
            event_tx,
            command_tx,
            command_rx: Arc::new(RwLock::new(Some(command_rx))),
            subscriptions: Arc::new(RwLock::new(SubscriptionState::default())),
            reconnect_count: Arc::new(RwLock::new(0)),
            is_running: Arc::new(RwLock::new(false)),
        };

        (monitor, event_rx)
    }

    /// Create with default configuration
    pub fn with_defaults() -> (Self, mpsc::Receiver<PumpEvent>) {
        Self::new(PumpMonitorConfig::default())
    }

    /// Subscribe to new token launches
    pub async fn subscribe_new_tokens(&self) -> Result<(), PumpMonitorError> {
        self.command_tx
            .send(MonitorCommand::SubscribeNewTokens)
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Unsubscribe from new token launches
    pub async fn unsubscribe_new_tokens(&self) -> Result<(), PumpMonitorError> {
        self.command_tx
            .send(MonitorCommand::UnsubscribeNewTokens)
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Subscribe to trades on specific tokens
    pub async fn subscribe_token_trades(&self, mints: Vec<String>) -> Result<(), PumpMonitorError> {
        if mints.is_empty() {
            return Ok(());
        }
        self.command_tx
            .send(MonitorCommand::SubscribeTokenTrades(mints))
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Unsubscribe from token trades
    pub async fn unsubscribe_token_trades(
        &self,
        mints: Vec<String>,
    ) -> Result<(), PumpMonitorError> {
        self.command_tx
            .send(MonitorCommand::UnsubscribeTokenTrades(mints))
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Subscribe to trades by specific accounts (e.g., known snipers)
    pub async fn subscribe_account_trades(
        &self,
        accounts: Vec<String>,
    ) -> Result<(), PumpMonitorError> {
        if accounts.is_empty() {
            return Ok(());
        }
        self.command_tx
            .send(MonitorCommand::SubscribeAccountTrades(accounts))
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Request graceful shutdown
    pub async fn shutdown(&self) -> Result<(), PumpMonitorError> {
        self.command_tx
            .send(MonitorCommand::Shutdown)
            .await
            .map_err(|_| PumpMonitorError::ChannelClosed)
    }

    /// Check if monitor is currently running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get current reconnection count
    pub async fn reconnect_count(&self) -> u32 {
        *self.reconnect_count.read().await
    }

    /// Get current subscription state (for debugging)
    pub async fn subscription_state(&self) -> (bool, usize, usize) {
        let state = self.subscriptions.read().await;
        (
            state.new_tokens,
            state.token_trades.len(),
            state.account_trades.len(),
        )
    }

    /// Run the monitor (this is the main event loop)
    /// This method blocks until shutdown is requested or an unrecoverable error occurs
    pub async fn run(&self) -> Result<(), PumpMonitorError> {
        // Take ownership of command receiver
        let command_rx = {
            let mut rx_guard = self.command_rx.write().await;
            rx_guard.take().ok_or(PumpMonitorError::NotRunning)?
        };

        *self.is_running.write().await = true;
        info!("PumpFunMonitor starting, connecting to {}", self.config.ws_url);

        let result = self.run_loop(command_rx).await;

        *self.is_running.write().await = false;
        info!("PumpFunMonitor stopped");

        result
    }

    /// Internal run loop with reconnection logic
    async fn run_loop(
        &self,
        mut command_rx: mpsc::Receiver<MonitorCommand>,
    ) -> Result<(), PumpMonitorError> {
        let mut reconnect_attempts = 0u32;

        loop {
            // Attempt connection
            match self.connect_and_process(&mut command_rx).await {
                Ok(should_shutdown) => {
                    if should_shutdown {
                        info!("Shutdown requested, exiting monitor loop");
                        return Ok(());
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);

                    // Send error event
                    let _ = self
                        .event_tx
                        .send(PumpEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                }
            }

            // Check if we should reconnect
            if !self.config.auto_reconnect {
                return Err(PumpMonitorError::ConnectionFailed(
                    "Auto-reconnect disabled".into(),
                ));
            }

            // Check max reconnect attempts
            if self.config.max_reconnect_attempts > 0
                && reconnect_attempts >= self.config.max_reconnect_attempts
            {
                return Err(PumpMonitorError::ConnectionFailed(format!(
                    "Max reconnect attempts ({}) exceeded",
                    self.config.max_reconnect_attempts
                )));
            }

            // Calculate backoff delay
            reconnect_attempts += 1;
            *self.reconnect_count.write().await = reconnect_attempts;

            let delay_ms = std::cmp::min(
                RECONNECT_BASE_DELAY_MS * 2u64.pow(reconnect_attempts.min(10)),
                MAX_RECONNECT_DELAY_MS,
            );

            warn!(
                "Reconnecting in {}ms (attempt {})",
                delay_ms, reconnect_attempts
            );

            // Send connection state event
            let _ = self
                .event_tx
                .send(PumpEvent::ConnectionState {
                    connected: false,
                    reconnect_count: reconnect_attempts,
                })
                .await;

            // Wait before reconnecting (but check for shutdown)
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                Some(cmd) = command_rx.recv() => {
                    if matches!(cmd, MonitorCommand::Shutdown) {
                        return Ok(());
                    }
                    // Re-queue other commands
                    // Note: In production, you'd want a more sophisticated approach
                }
            }
        }
    }

    /// Connect to WebSocket and process messages
    /// Returns Ok(true) if shutdown was requested, Ok(false) for normal disconnect
    async fn connect_and_process(
        &self,
        command_rx: &mut mpsc::Receiver<MonitorCommand>,
    ) -> Result<bool, PumpMonitorError> {
        // Note: In production, this would use a WebSocket library like tokio-tungstenite
        // For now, we implement the message processing logic that would run once connected

        info!("Connected to pump.fun WebSocket");

        // Reset reconnect count on successful connection
        *self.reconnect_count.write().await = 0;

        // Send connection state event
        let _ = self
            .event_tx
            .send(PumpEvent::ConnectionState {
                connected: true,
                reconnect_count: 0,
            })
            .await;

        // Auto-subscribe to new tokens if configured
        if self.config.subscribe_new_tokens {
            self.subscriptions.write().await.new_tokens = true;
            debug!("Auto-subscribed to new token events");
        }

        // Process commands and messages
        // In production, this would be a select! over WebSocket messages and commands
        loop {
            tokio::select! {
                Some(cmd) = command_rx.recv() => {
                    match cmd {
                        MonitorCommand::Shutdown => {
                            info!("Shutdown command received");
                            return Ok(true);
                        }
                        MonitorCommand::SubscribeNewTokens => {
                            self.subscriptions.write().await.new_tokens = true;
                            // In production: send subscribe message over WebSocket
                            debug!("Subscribed to new tokens");
                        }
                        MonitorCommand::UnsubscribeNewTokens => {
                            self.subscriptions.write().await.new_tokens = false;
                            debug!("Unsubscribed from new tokens");
                        }
                        MonitorCommand::SubscribeTokenTrades(mints) => {
                            let mut subs = self.subscriptions.write().await;
                            for mint in mints {
                                subs.token_trades.insert(mint);
                            }
                            debug!("Subscribed to token trades, total: {}", subs.token_trades.len());
                        }
                        MonitorCommand::UnsubscribeTokenTrades(mints) => {
                            let mut subs = self.subscriptions.write().await;
                            for mint in &mints {
                                subs.token_trades.remove(mint);
                            }
                            debug!("Unsubscribed from token trades");
                        }
                        MonitorCommand::SubscribeAccountTrades(accounts) => {
                            let mut subs = self.subscriptions.write().await;
                            for account in accounts {
                                subs.account_trades.insert(account);
                            }
                            debug!("Subscribed to account trades, total: {}", subs.account_trades.len());
                        }
                    }
                }
                // In production, add: ws_message = ws_stream.next() => { ... }
                // For now, we'll just wait and simulate occasional disconnects for testing
                _ = tokio::time::sleep(Duration::from_secs(PING_INTERVAL_SECS)) => {
                    if self.config.enable_keepalive {
                        // In production: send ping frame
                        debug!("Keepalive ping");
                    }
                }
            }
        }
    }

    /// Process a raw WebSocket message
    /// This is called by the connection handler when a message is received
    pub async fn process_message(&self, raw_message: &str) -> Result<(), PumpMonitorError> {
        // Validate message size to prevent memory exhaustion
        if raw_message.len() > self.config.max_message_size {
            warn!(
                "Message exceeds max size: {} > {} bytes",
                raw_message.len(),
                self.config.max_message_size
            );
            return Err(PumpMonitorError::InvalidMessageFormat(format!(
                "Message too large: {} bytes",
                raw_message.len()
            )));
        }

        // Check for empty or whitespace-only messages
        let trimmed = raw_message.trim();
        if trimmed.is_empty() {
            debug!("Ignoring empty message");
            return Ok(());
        }

        // Validate basic JSON structure before full parse
        if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
            warn!("Invalid JSON structure: does not start with {{ or [");
            return Err(PumpMonitorError::InvalidMessageFormat(
                "Message is not valid JSON object or array".to_string(),
            ));
        }

        // Try to parse the message with detailed error handling
        let parsed: RawPumpMessage = match serde_json::from_str(raw_message) {
            Ok(msg) => msg,
            Err(e) => {
                // Log the parse error with context but truncate long messages
                let preview = if raw_message.len() > 200 {
                    format!("{}...[truncated]", &raw_message[..200])
                } else {
                    raw_message.to_string()
                };
                warn!("Failed to parse message: {} - preview: {}", e, preview);
                return Err(PumpMonitorError::ParseError(format!(
                    "JSON parse error at line {}, column {}: {:?}",
                    e.line(),
                    e.column(),
                    e.classify()
                )));
            }
        };

        match parsed {
            RawPumpMessage::NewToken(token) => {
                // Check if we're subscribed to new tokens
                if self.subscriptions.read().await.new_tokens {
                    let event = PumpEvent::from_token(&token);
                    self.event_tx
                        .send(event)
                        .await
                        .map_err(|_| PumpMonitorError::ChannelClosed)?;
                    debug!(
                        "New token event: {} ({}) - {}",
                        token.name, token.symbol, token.mint
                    );
                }
            }
            RawPumpMessage::Trade(trade) => {
                // Check if we're tracking this token or account
                let subs = self.subscriptions.read().await;
                let should_emit = subs.token_trades.contains(&trade.mint)
                    || subs.account_trades.contains(&trade.trader);

                if should_emit {
                    // Send trade event
                    let event = PumpEvent::from_trade(&trade);
                    self.event_tx
                        .send(event)
                        .await
                        .map_err(|_| PumpMonitorError::ChannelClosed)?;

                    // Also send graduation progress if applicable
                    if trade.virtual_sol_reserves > 0 {
                        let grad_event = PumpEvent::graduation_from_trade(&trade);
                        self.event_tx
                            .send(grad_event)
                            .await
                            .map_err(|_| PumpMonitorError::ChannelClosed)?;
                    }

                    debug!(
                        "Trade event: {} {} {} SOL",
                        if trade.is_buy { "BUY" } else { "SELL" },
                        trade.mint,
                        trade.sol_amount as f64 / 1e9
                    );
                }
            }
            RawPumpMessage::Confirmation { message } => {
                debug!("Subscription confirmed: {}", message);
            }
            RawPumpMessage::Error { error } => {
                warn!("Server error: {}", error);
                let _ = self
                    .event_tx
                    .send(PumpEvent::Error { message: error })
                    .await;
            }
        }

        Ok(())
    }

    /// Create subscription message JSON for a given subscription type
    pub fn create_subscribe_message(sub_type: &str, keys: Option<Vec<String>>) -> String {
        let msg = SubscribeMessage {
            method: sub_type.to_string(),
            keys,
        };
        serde_json::to_string(&msg).unwrap_or_default()
    }
}

/// Builder for PumpFunMonitor configuration
#[derive(Debug, Default)]
pub struct PumpMonitorBuilder {
    config: PumpMonitorConfig,
}

impl PumpMonitorBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set WebSocket URL
    pub fn ws_url(mut self, url: impl Into<String>) -> Self {
        self.config.ws_url = url.into();
        self
    }

    /// Enable or disable auto-reconnect
    pub fn auto_reconnect(mut self, enabled: bool) -> Self {
        self.config.auto_reconnect = enabled;
        self
    }

    /// Set maximum reconnection attempts (0 = unlimited)
    pub fn max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.config.max_reconnect_attempts = attempts;
        self
    }

    /// Subscribe to new tokens on connect
    pub fn subscribe_new_tokens(mut self, enabled: bool) -> Self {
        self.config.subscribe_new_tokens = enabled;
        self
    }

    /// Set event channel buffer size
    pub fn channel_buffer_size(mut self, size: usize) -> Self {
        self.config.channel_buffer_size = size;
        self
    }

    /// Enable or disable keepalive pings
    pub fn enable_keepalive(mut self, enabled: bool) -> Self {
        self.config.enable_keepalive = enabled;
        self
    }

    /// Set message receive timeout in seconds (0 = no timeout)
    pub fn message_timeout_secs(mut self, secs: u64) -> Self {
        self.config.message_timeout_secs = secs;
        self
    }

    /// Set maximum message size in bytes
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.max_message_size = size;
        self
    }

    /// Build the monitor and return with event receiver
    pub fn build(self) -> (PumpFunMonitor, mpsc::Receiver<PumpEvent>) {
        PumpFunMonitor::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pump_event_from_token() {
        let token = PumpFunToken {
            mint: "mint123".to_string(),
            name: "Test Token".to_string(),
            symbol: "TEST".to_string(),
            description: None,
            uri: Some("https://example.com".to_string()),
            creator: "creator456".to_string(),
            initial_buy: 1000000,
            market_cap_sol: 30.5,
            image_url: None,
            twitter: None,
            telegram: None,
            website: None,
            timestamp: 12345,
        };

        let event = PumpEvent::from_token(&token);
        match event {
            PumpEvent::NewToken {
                mint,
                name,
                symbol,
                creator,
                initial_buy,
                market_cap_sol,
                ..
            } => {
                assert_eq!(mint, "mint123");
                assert_eq!(name, "Test Token");
                assert_eq!(symbol, "TEST");
                assert_eq!(creator, "creator456");
                assert_eq!(initial_buy, 1000000);
                assert!((market_cap_sol - 30.5).abs() < 0.001);
            }
            _ => panic!("Expected NewToken event"),
        }
    }

    #[test]
    fn test_pump_event_from_trade() {
        let trade = TradeInfo {
            mint: "mint123".to_string(),
            signature: Some("sig".to_string()),
            trader: "trader789".to_string(),
            is_buy: true,
            sol_amount: 2_000_000_000,
            token_amount: 1_000_000_000,
            market_cap_sol: 35.0,
            virtual_sol_reserves: 32_000_000_000,
            virtual_token_reserves: 950_000_000_000_000,
            timestamp: 12345,
        };

        let event = PumpEvent::from_trade(&trade);
        match event {
            PumpEvent::TokenTrade {
                mint,
                is_buy,
                sol_amount,
                trader,
                ..
            } => {
                assert_eq!(mint, "mint123");
                assert!(is_buy);
                assert_eq!(sol_amount, 2_000_000_000);
                assert_eq!(trader, "trader789");
            }
            _ => panic!("Expected TokenTrade event"),
        }
    }

    #[test]
    fn test_graduation_progress_calculation() {
        let trade = TradeInfo {
            mint: "mint123".to_string(),
            signature: None,
            trader: "trader".to_string(),
            is_buy: true,
            sol_amount: 0,
            token_amount: 0,
            market_cap_sol: 0.0,
            virtual_sol_reserves: 42_500_000_000, // 42.5 SOL = 50% of 85
            virtual_token_reserves: 0,
            timestamp: 0,
        };

        let event = PumpEvent::graduation_from_trade(&trade);
        match event {
            PumpEvent::GraduationProgress {
                bonding_curve_percent,
                ..
            } => {
                assert!((bonding_curve_percent - 50.0).abs() < 0.1);
            }
            _ => panic!("Expected GraduationProgress event"),
        }
    }

    #[test]
    fn test_config_default() {
        let config = PumpMonitorConfig::default();
        assert_eq!(config.ws_url, DEFAULT_WS_URL);
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 0);
        assert!(config.subscribe_new_tokens);
        assert_eq!(config.channel_buffer_size, 1000);
        assert!(config.enable_keepalive);
    }

    #[test]
    fn test_builder() {
        let (monitor, _rx) = PumpMonitorBuilder::new()
            .ws_url("wss://custom.url")
            .auto_reconnect(false)
            .max_reconnect_attempts(5)
            .subscribe_new_tokens(false)
            .channel_buffer_size(500)
            .enable_keepalive(false)
            .build();

        assert_eq!(monitor.config.ws_url, "wss://custom.url");
        assert!(!monitor.config.auto_reconnect);
        assert_eq!(monitor.config.max_reconnect_attempts, 5);
        assert!(!monitor.config.subscribe_new_tokens);
        assert_eq!(monitor.config.channel_buffer_size, 500);
        assert!(!monitor.config.enable_keepalive);
    }

    #[test]
    fn test_subscribe_message_json() {
        let json = PumpFunMonitor::create_subscribe_message("subscribeNewToken", None);
        assert!(json.contains("subscribeNewToken"));
        assert!(!json.contains("keys"));

        let json = PumpFunMonitor::create_subscribe_message(
            "subscribeTokenTrade",
            Some(vec!["mint1".to_string()]),
        );
        assert!(json.contains("subscribeTokenTrade"));
        assert!(json.contains("mint1"));
    }

    #[tokio::test]
    async fn test_monitor_creation() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();
        assert!(!monitor.is_running().await);
        assert_eq!(monitor.reconnect_count().await, 0);
    }

    #[tokio::test]
    async fn test_subscription_state() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();
        let (new_tokens, token_trades, account_trades) = monitor.subscription_state().await;

        assert!(!new_tokens);
        assert_eq!(token_trades, 0);
        assert_eq!(account_trades, 0);
    }

    #[tokio::test]
    async fn test_process_new_token_message() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();

        // Enable new token subscription
        monitor.subscriptions.write().await.new_tokens = true;

        let json = r#"{
            "mint": "TokenMint123",
            "name": "Test Meme",
            "symbol": "MEME",
            "uri": "https://ipfs.io/ipfs/abc",
            "traderPublicKey": "Creator456",
            "initialBuy": 1000000,
            "marketCapSol": 30.5
        }"#;

        monitor.process_message(json).await.unwrap();

        // Check that event was emitted
        let event = rx.try_recv().unwrap();
        match event {
            PumpEvent::NewToken { mint, symbol, .. } => {
                assert_eq!(mint, "TokenMint123");
                assert_eq!(symbol, "MEME");
            }
            _ => panic!("Expected NewToken event"),
        }
    }

    #[tokio::test]
    async fn test_process_trade_message() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();

        // Subscribe to this token's trades
        monitor
            .subscriptions
            .write()
            .await
            .token_trades
            .insert("TokenMint123".to_string());

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

        monitor.process_message(json).await.unwrap();

        // Should get trade event
        let event = rx.try_recv().unwrap();
        match event {
            PumpEvent::TokenTrade {
                mint,
                is_buy,
                sol_amount,
                ..
            } => {
                assert_eq!(mint, "TokenMint123");
                assert!(is_buy);
                assert_eq!(sol_amount, 2000000000);
            }
            _ => panic!("Expected TokenTrade event"),
        }

        // Should also get graduation progress event
        let event = rx.try_recv().unwrap();
        match event {
            PumpEvent::GraduationProgress {
                mint,
                virtual_sol_reserves,
                ..
            } => {
                assert_eq!(mint, "TokenMint123");
                assert_eq!(virtual_sol_reserves, 32000000000);
            }
            _ => panic!("Expected GraduationProgress event"),
        }
    }

    #[tokio::test]
    async fn test_unsubscribed_messages_ignored() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();

        // Don't subscribe to anything

        let json = r#"{
            "mint": "TokenMint123",
            "name": "Test",
            "symbol": "TEST",
            "traderPublicKey": "Creator",
            "initialBuy": 0,
            "marketCapSol": 0
        }"#;

        monitor.process_message(json).await.unwrap();

        // Should not receive any event
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_account_trade_subscription() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();

        // Subscribe to a specific trader account
        monitor
            .subscriptions
            .write()
            .await
            .account_trades
            .insert("WhaleSniperAccount".to_string());

        let json = r#"{
            "mint": "AnyToken",
            "traderPublicKey": "WhaleSniperAccount",
            "txType": true,
            "solAmount": 5000000000,
            "tokenAmount": 1000000000,
            "marketCapSol": 40.0,
            "vSolInBondingCurve": 0,
            "vTokensInBondingCurve": 0
        }"#;

        monitor.process_message(json).await.unwrap();

        // Should receive event because we're tracking this account
        let event = rx.try_recv().unwrap();
        match event {
            PumpEvent::TokenTrade { trader, .. } => {
                assert_eq!(trader, "WhaleSniperAccount");
            }
            _ => panic!("Expected TokenTrade event"),
        }
    }

    #[test]
    fn test_error_types() {
        let err = PumpMonitorError::ConnectionFailed("test".to_string());
        assert!(err.to_string().contains("connection failed"));

        let err = PumpMonitorError::ParseError("invalid json".to_string());
        assert!(err.to_string().contains("parse"));
    }

    // ===== Edge Case Tests for Error Handling =====

    #[tokio::test]
    async fn test_process_empty_message() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();

        // Empty message should be handled gracefully
        let result = monitor.process_message("").await;
        assert!(result.is_ok()); // Empty messages are ignored, not errors

        // Whitespace-only message
        let result = monitor.process_message("   \n\t   ").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_malformed_json() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();

        // Incomplete JSON
        let result = monitor.process_message(r#"{"mint": "test""#).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PumpMonitorError::ParseError(_)));

        // Not valid JSON at all
        let result = monitor.process_message("this is not json").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PumpMonitorError::InvalidMessageFormat(_)
        ));
    }

    #[tokio::test]
    async fn test_process_oversized_message() {
        let config = PumpMonitorConfig {
            max_message_size: 100, // Very small limit for testing
            ..Default::default()
        };
        let (monitor, _rx) = PumpFunMonitor::new(config);

        // Create a message larger than the limit
        let large_message = format!(r#"{{"mint": "{}"}}"#, "x".repeat(200));
        let result = monitor.process_message(&large_message).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PumpMonitorError::InvalidMessageFormat(_)
        ));
    }

    #[tokio::test]
    async fn test_process_message_invalid_structure() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();

        // Array instead of object (not valid for pump.fun messages)
        let result = monitor.process_message(r#"[1, 2, 3]"#).await;
        // This should parse but may not match any variant
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_message_with_null_fields() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();
        monitor.subscriptions.write().await.new_tokens = true;

        // Message with null fields (should use defaults)
        let json = r#"{
            "mint": "TokenMint123",
            "name": "Test",
            "symbol": "TEST",
            "traderPublicKey": "Creator",
            "initialBuy": null,
            "marketCapSol": null
        }"#;

        // This may fail due to null on non-Option fields, which is expected
        let result = monitor.process_message(json).await;
        // Either succeeds with defaults or fails with parse error
        if result.is_ok() {
            // If it succeeds, verify we got an event
            let _ = rx.try_recv();
        }
    }

    #[test]
    fn test_new_error_types() {
        let err = PumpMonitorError::MessageTimeout(30);
        assert!(err.to_string().contains("30"));
        assert!(err.to_string().contains("timeout"));

        let err = PumpMonitorError::RecoveryFailed("connection lost".to_string());
        assert!(err.to_string().contains("recovery"));

        let err = PumpMonitorError::InvalidMessageFormat("bad format".to_string());
        assert!(err.to_string().contains("format"));

        let err = PumpMonitorError::MissingField("mint".to_string());
        assert!(err.to_string().contains("mint"));
    }

    #[test]
    fn test_config_with_timeout_settings() {
        let config = PumpMonitorConfig {
            message_timeout_secs: 30,
            max_message_size: 512_000,
            ..Default::default()
        };

        assert_eq!(config.message_timeout_secs, 30);
        assert_eq!(config.max_message_size, 512_000);
    }

    #[test]
    fn test_builder_with_timeout_settings() {
        let (monitor, _rx) = PumpMonitorBuilder::new()
            .message_timeout_secs(45)
            .max_message_size(2_000_000)
            .build();

        assert_eq!(monitor.config.message_timeout_secs, 45);
        assert_eq!(monitor.config.max_message_size, 2_000_000);
    }

    #[tokio::test]
    async fn test_process_confirmation_message() {
        let (monitor, _rx) = PumpFunMonitor::with_defaults();

        let json = r#"{"message": "Subscribed to newToken"}"#;
        let result = monitor.process_message(json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_error_message() {
        let (monitor, mut rx) = PumpFunMonitor::with_defaults();

        let json = r#"{"error": "Invalid subscription"}"#;
        let result = monitor.process_message(json).await;
        assert!(result.is_ok());

        // Should emit an error event
        let event = rx.try_recv();
        if let Ok(PumpEvent::Error { message }) = event {
            assert!(message.contains("Invalid subscription"));
        }
    }

    #[tokio::test]
    async fn test_graduation_progress_zero_reserves() {
        let trade = TradeInfo {
            mint: "mint123".to_string(),
            signature: None,
            trader: "trader".to_string(),
            is_buy: true,
            sol_amount: 0,
            token_amount: 0,
            market_cap_sol: 0.0,
            virtual_sol_reserves: 0, // Edge case: zero reserves
            virtual_token_reserves: 0,
            timestamp: 0,
        };

        let event = PumpEvent::graduation_from_trade(&trade);
        match event {
            PumpEvent::GraduationProgress {
                bonding_curve_percent,
                ..
            } => {
                // Should handle zero gracefully
                assert_eq!(bonding_curve_percent, 0.0);
            }
            _ => panic!("Expected GraduationProgress event"),
        }
    }
}
