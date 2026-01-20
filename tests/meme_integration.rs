//! Meme Coin Launcher Integration Tests
//!
//! Integration tests that verify the meme coin trading components work together:
//! 1. PumpFunMonitor -> LaunchSniperStrategy flow
//! 2. LaunchSniperStrategy -> RugDetector safety checks
//! 3. MemeOrchestrator coordination of all components
//!
//! All tests are deterministic (no real network calls) and use mock data.

// Import the components we're testing
use butters::adapters::pump_fun::{BondingCurveState, PumpFunToken, TradeInfo};
use butters::domain::rug_detector::{
    HolderInfo, LiquidityInfo, RiskLevel, RugDetector, RugWarning, TokenAnalysisData,
    TokenSafetyReport,
};
use butters::strategy::launch_sniper::{LaunchSignal, LaunchSniperConfig, LaunchSniperStrategy};

// ============================================================================
// Test Fixtures
// ============================================================================

/// Create a mock PumpFunToken for testing
fn create_mock_pump_token(mint: &str, symbol: &str, creator: &str) -> PumpFunToken {
    PumpFunToken {
        mint: mint.to_string(),
        name: format!("{} Token", symbol),
        symbol: symbol.to_string(),
        description: Some("A test meme token".to_string()),
        uri: Some("https://ipfs.io/ipfs/test123".to_string()),
        creator: creator.to_string(),
        initial_buy: 1_000_000,
        market_cap_sol: 25.0,
        image_url: Some("https://example.com/image.png".to_string()),
        twitter: Some("@testtoken".to_string()),
        telegram: Some("t.me/testtoken".to_string()),
        website: Some("https://testtoken.io".to_string()),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    }
}

/// Create a mock TradeInfo for testing
fn create_mock_trade(
    mint: &str,
    is_buy: bool,
    sol_amount_sol: f64,
    virtual_sol_lamports: u64,
) -> TradeInfo {
    TradeInfo {
        mint: mint.to_string(),
        signature: Some("sig123abc".to_string()),
        trader: "TraderWallet123".to_string(),
        is_buy,
        sol_amount: (sol_amount_sol * 1_000_000_000.0) as u64,
        token_amount: 1_000_000_000,
        market_cap_sol: virtual_sol_lamports as f64 / 1_000_000_000.0,
        virtual_sol_reserves: virtual_sol_lamports,
        virtual_token_reserves: 900_000_000_000_000,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    }
}

/// Create a mock BondingCurveState
fn create_mock_bonding_curve(
    mint: &str,
    real_sol_lamports: u64,
    complete: bool,
) -> BondingCurveState {
    BondingCurveState {
        mint: mint.to_string(),
        virtual_sol_reserves: 30_000_000_000,
        virtual_token_reserves: 1_000_000_000_000_000,
        real_sol_reserves: real_sol_lamports,
        real_token_reserves: 500_000_000_000_000,
        token_total_supply: 1_000_000_000_000_000,
        complete,
    }
}

/// Create mock token analysis data for rug detector
fn create_mock_token_analysis_data(mint: &str, has_authorities: bool) -> TokenAnalysisData {
    TokenAnalysisData {
        mint: mint.to_string(),
        mint_authority: if has_authorities {
            Some("MintAuth123".to_string())
        } else {
            None
        },
        freeze_authority: if has_authorities {
            Some("FreezeAuth123".to_string())
        } else {
            None
        },
        supply: 1_000_000_000_000_000,
        decimals: 6,
        name: Some("Safe Token".to_string()),
        symbol: Some("SAFE".to_string()),
        uri: None,
        created_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 100 * 3600, // 100 hours ago
        ),
        extensions: Vec::new(),
        transfer_fee_bps: None,
    }
}

/// Create mock holder data with varied percentages to avoid sybil detection
fn create_mock_holders(count: usize, max_single_pct: f64) -> Vec<HolderInfo> {
    let mut holders = Vec::new();
    let remaining_pct = 100.0 - max_single_pct;

    // Calculate a base percentage that accounts for all holders
    // Use decreasing percentages to create a realistic distribution
    // and avoid triggering sybil detection or concentration warnings
    for i in 0..count {
        let percentage = if i == 0 {
            max_single_pct
        } else {
            // Create a decreasing percentage distribution
            // Larger indices = smaller percentages (more realistic distribution)
            let base = remaining_pct / (count as f64 * 1.5);
            let decay = 1.0 - (i as f64 / count as f64 * 0.7);
            (base * decay).max(0.01) // Min 0.01%
        };

        holders.push(HolderInfo {
            address: format!("Holder{}", i),
            amount: (1_000_000_000.0 * percentage / max_single_pct) as u64 + 1000,
            percentage,
            is_creator: i == 0,
            is_known_exchange: i == 1, // Mark one as known exchange
        });
    }
    holders
}

/// Create mock liquidity info
fn create_mock_liquidity(liquidity_usd: f64, lp_burned: bool) -> LiquidityInfo {
    LiquidityInfo {
        pool_address: "Pool123".to_string(),
        lp_mint: "LPMint123".to_string(),
        liquidity_usd,
        token_amount: 500_000_000_000,
        quote_amount: 1_000_000_000,
        lp_burned,
        lp_holder: if lp_burned {
            None
        } else {
            Some("Creator123".to_string())
        },
        lp_locked_percentage: if lp_burned { 100.0 } else { 0.0 },
    }
}

/// Create a launch sniper config for testing with relaxed parameters
fn create_test_sniper_config() -> LaunchSniperConfig {
    LaunchSniperConfig {
        min_bonding_curve_percent: 0.85,
        max_bonding_curve_percent: 0.99,
        min_unique_holders: 50,
        max_creator_holding_percent: 0.05,
        min_liquidity_sol: 10.0,
        max_token_age_minutes: 60,
        entry_size_usdc: 50.0,
        take_profit_percent: 0.50,
        stop_loss_percent: 0.20,
        max_hold_minutes: 30,
        min_fill_rate_per_minute: 0.0, // Disabled for unit tests
        min_holder_growth_rate: 0.0,   // Disabled for unit tests
        max_concurrent_positions: 1,
        max_daily_entries: 10,
        max_daily_loss_usdc: 100.0,
        cooldown_seconds: 0, // No cooldown for tests
    }
}

// ============================================================================
// Test Module: PumpFunToken -> LaunchSniperStrategy Flow
// ============================================================================

mod pump_to_sniper_flow {
    use super::*;

    /// Test: New token creates a graduation candidate in strategy
    #[test]
    fn test_new_token_creates_candidate() {
        let token = create_mock_pump_token("Mint123", "TEST", "Creator456");

        // Verify token was created correctly
        assert_eq!(token.mint, "Mint123");
        assert_eq!(token.symbol, "TEST");
        assert_eq!(token.creator, "Creator456");
        assert!((token.market_cap_sol - 25.0).abs() < 0.001);

        // Create strategy and track the candidate
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate(token.mint.clone(), token.symbol.clone());

        // Verify candidate was created
        let candidate = strategy.get_candidate(&token.mint);
        assert!(candidate.is_some());
        assert_eq!(candidate.unwrap().symbol, "TEST");
    }

    /// Test: Trade events update bonding curve progress via strategy update
    #[test]
    fn test_trade_updates_bonding_curve() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Simulate multiple trade events updating the bonding curve
        let trades = vec![
            create_mock_trade("Mint123", true, 1.0, 40_000_000_000),
            create_mock_trade("Mint123", true, 2.0, 50_000_000_000),
            create_mock_trade("Mint123", true, 3.0, 70_000_000_000),
        ];

        // Update candidate with each trade via the public API
        for (i, trade) in trades.iter().enumerate() {
            let bonding_percent = trade.virtual_sol_reserves as f64 / 85_000_000_000.0;
            let price = 0.001 + (i as f64) * 0.0001;
            let holder_count = 50 + (i as u32) * 10;

            strategy
                .update_candidate(
                    &trade.mint,
                    bonding_percent,
                    price,
                    holder_count,
                    0.02,  // creator holding
                    100.0, // liquidity SOL
                )
                .unwrap();
        }

        // Verify the candidate has been updated
        let candidate = strategy.get_candidate("Mint123").unwrap();
        assert!(candidate.current_bonding_percent().is_some());
        let bonding_pct = candidate.current_bonding_percent().unwrap();
        assert!(bonding_pct > 0.8); // Should be near graduation
    }

    /// Test: Graduation progress is calculated correctly from bonding curve state
    #[test]
    fn test_graduation_progress_calculation() {
        // 42.5 SOL = 50% of 85 SOL graduation threshold
        let curve = create_mock_bonding_curve("Mint123", 42_500_000_000, false);
        let progress = curve.graduation_progress();
        assert!((progress - 50.0).abs() < 0.1);
    }

    /// Test: Near-graduation detection works correctly
    #[test]
    fn test_near_graduation_detection() {
        let curve = create_mock_bonding_curve("Mint123", 70_000_000_000, false);
        assert!(curve.is_near_graduation());

        let early_curve = create_mock_bonding_curve("Mint123", 40_000_000_000, false);
        assert!(!early_curve.is_near_graduation());
    }

    /// Test: Near-graduation token triggers entry evaluation
    #[test]
    fn test_near_graduation_triggers_entry_evaluation() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Set up candidate to be near graduation with all safety checks passed
        // Use the public update_candidate API
        for i in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let bonding_pct = 0.88 + (i as f64) * 0.02;
            strategy
                .update_candidate(
                    "Mint123",
                    bonding_pct,
                    0.001,
                    100, // holder_count
                    0.02, // creator holding (2%)
                    100.0, // liquidity SOL
                )
                .unwrap();
        }

        // Evaluate entry - should produce some signal
        let signal = strategy.evaluate_entry("Mint123");
        assert!(signal.is_some());
    }
}

// ============================================================================
// Test Module: LaunchSniperStrategy -> RugDetector Safety Checks
// ============================================================================

mod sniper_to_rug_detector_flow {
    use super::*;

    /// Test: Safe token passes rug detector and allows entry
    #[test]
    fn test_safe_token_allows_entry() {
        // Create rug detector with pump.fun sniper config (lenient)
        let mut detector = RugDetector::pump_fun_sniper();

        // For pump_fun_sniper config, require_revoked_mint=true, so NO authorities
        let token_data = create_mock_token_analysis_data("Mint123", false); // No authorities
        // pump_fun_sniper allows max_single_holder_percent=10.0, and min_holder_count=10
        let holders = create_mock_holders(20, 8.0); // 20 holders, max 8% single - passes pump_fun config
        // pump_fun_sniper requires min_liquidity_usd=1_000
        let liquidity = create_mock_liquidity(5_000.0, false); // >1000 USD, LP not burned (allowed by pump_fun_sniper)

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        // With pump_fun_sniper config these should pass
        assert!(report.is_safe(), "Report warnings: {:?}", report.warnings);
        assert!(report.risk_level.is_tradeable());
        assert_eq!(report.risk_level.position_size_multiplier(), 1.0);
    }

    /// Test: Token with active mint authority is flagged
    #[test]
    fn test_active_mint_authority_flagged() {
        let mut detector = RugDetector::new();

        let token_data = create_mock_token_analysis_data("Mint123", true); // Has authorities
        let holders = create_mock_holders(200, 1.5);
        let liquidity = create_mock_liquidity(50_000.0, true);

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(!report.is_safe());
        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::MintAuthorityActive { .. })));
    }

    /// Test: High creator concentration is flagged
    #[test]
    fn test_high_creator_concentration_flagged() {
        let mut detector = RugDetector::new();

        let token_data = create_mock_token_analysis_data("Mint123", false);
        let holders = create_mock_holders(100, 30.0); // 30% in one wallet
        let liquidity = create_mock_liquidity(50_000.0, true);

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::HighCreatorHolding { .. })));
    }

    /// Test: Low liquidity is flagged
    #[test]
    fn test_low_liquidity_flagged() {
        let mut detector = RugDetector::new();

        let token_data = create_mock_token_analysis_data("Mint123", false);
        let holders = create_mock_holders(200, 1.5);
        let liquidity = create_mock_liquidity(500.0, true); // Very low

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::LowLiquidity { .. })));
    }

    /// Test: Token with rug warnings is blocked from entry
    #[test]
    fn test_rug_warnings_block_entry() {
        let mut detector = RugDetector::new();

        // Create a clearly dangerous token
        let mut token_data = create_mock_token_analysis_data("Mint123", true);
        token_data.name = Some("HoneypotScam".to_string());
        token_data.extensions = vec!["TransferHook".to_string()];

        let holders = create_mock_holders(20, 50.0); // Highly concentrated
        let mut liquidity = create_mock_liquidity(100.0, false);
        liquidity.lp_locked_percentage = 0.0;

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert_eq!(report.risk_level, RiskLevel::Critical);
        assert!(report.risk_level.should_block());
        assert!(!report.is_safe());
        assert!(report.warnings.len() >= 3); // Multiple warnings
    }

    /// Test: Integration - rug detector result affects sniper decision
    #[test]
    fn test_rug_detector_gates_sniper_entry() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        let mut detector = RugDetector::new();

        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Analyze token with rug detector
        let token_data = create_mock_token_analysis_data("Mint123", true); // Dangerous
        let holders = create_mock_holders(50, 25.0); // Bad distribution
        let liquidity = create_mock_liquidity(5_000.0, false);

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(!report.is_safe());

        // Update candidate with data that reflects rug issues
        // High creator holding = 25% = 0.25 which exceeds max of 0.05
        strategy
            .update_candidate(
                "Mint123",
                0.90, // bonding percent
                0.001, // price
                50,   // holder count
                0.25, // creator holding - HIGH
                33.0, // liquidity SOL
            )
            .unwrap();

        // Evaluate entry - should be Hold due to failed safety checks
        let signal = strategy.evaluate_entry("Mint123");
        assert_eq!(signal, Some(LaunchSignal::Hold));
    }
}

// ============================================================================
// Test Module: Exit Flow
// ============================================================================

mod exit_flow {
    use super::*;

    /// Test: Stop loss triggers exit
    #[test]
    fn test_stop_loss_triggers_exit() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Setup candidate with bonding curve data
        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Create a position via confirm_entry
        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();

        // Price drops 25% (beyond 20% stop loss)
        strategy.update_position_price("Mint123", 0.00075);

        let signal = strategy.evaluate_exit("Mint123");
        assert_eq!(signal, Some(LaunchSignal::StopLoss));
    }

    /// Test: Take profit triggers exit
    #[test]
    fn test_take_profit_triggers_exit() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();

        // Price rises 50% (at take profit level)
        strategy.update_position_price("Mint123", 0.0015);

        let signal = strategy.evaluate_exit("Mint123");
        assert_eq!(signal, Some(LaunchSignal::TakeProfit));
    }

    /// Test: Exit signal updates position state via confirm_exit
    #[test]
    fn test_exit_clears_position() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Set up candidate with bonding curve data
        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Confirm entry
        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();
        assert_eq!(strategy.get_positions().len(), 1);

        // Confirm exit
        let pnl = strategy.confirm_exit("Mint123", 0.0015);
        assert!(pnl.is_some());
        assert!(pnl.unwrap() > 40.0); // Should be ~50% profit
        assert!(strategy.get_positions().is_empty());
    }

    /// Test: Momentum fade triggers exit when giving back gains
    #[test]
    fn test_momentum_fade_triggers_exit() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();

        // MomentumFade triggers when:
        // 1. highest_price > entry_price * 1.1 (at least 10% up at some point)
        // 2. drawdown_percent > 0.3 (given back 30% from high)
        // So we need to go up >10%, then drop >30% from the high
        strategy.update_position_price("Mint123", 0.0015); // 50% up (highest)
        strategy.update_position_price("Mint123", 0.00105); // Now at 0.00105, which is 30% below 0.0015

        let signal = strategy.evaluate_exit("Mint123");
        // Should be MomentumFade (we're still up 5% from entry but gave back 30% from high)
        assert_eq!(signal, Some(LaunchSignal::MomentumFade));
    }
}

// ============================================================================
// Test Module: Daily Limits
// ============================================================================

mod daily_limits {
    use super::*;

    /// Test: Daily entry limit enforcement
    #[test]
    fn test_daily_entry_limit_enforced() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        // Exhaust daily entries by setting up and confirming many entries/exits
        strategy.track_candidate("Mint0".to_string(), "TEST0".to_string());
        strategy
            .update_candidate("Mint0", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Simulate 10 entries (max daily)
        for i in 0..10 {
            let mint = format!("Mint{}", i);
            if i > 0 {
                strategy.track_candidate(mint.clone(), format!("TEST{}", i));
                strategy
                    .update_candidate(&mint, 0.90, 0.001, 100, 0.02, 100.0)
                    .unwrap();
            }
            strategy.confirm_entry(&mint, 0.001, 1_000_000, 50.0).unwrap();
            strategy.confirm_exit(&mint, 0.001); // Break even
        }

        // Daily entries should be at max
        let stats = strategy.daily_stats();
        assert_eq!(stats.entries, 10);
        assert!(!strategy.can_trade());

        // New candidate should be blocked
        strategy.track_candidate("NewMint".to_string(), "NEW".to_string());
        strategy
            .update_candidate("NewMint", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        let signal = strategy.evaluate_entry("NewMint");
        assert_eq!(signal, Some(LaunchSignal::Hold));
    }

    /// Test: Daily loss limit enforcement
    #[test]
    fn test_daily_loss_limit_enforced() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        // Create positions that lose money to exceed daily loss limit
        for i in 0..3 {
            let mint = format!("Mint{}", i);
            strategy.track_candidate(mint.clone(), format!("TEST{}", i));
            strategy
                .update_candidate(&mint, 0.90, 0.001, 100, 0.02, 100.0)
                .unwrap();
            strategy.confirm_entry(&mint, 0.001, 1_000_000, 50.0).unwrap();
            // Exit at 75% loss each time = -$37.50 per trade
            // After 3 trades = -$112.50, exceeding -$100 limit
            strategy.confirm_exit(&mint, 0.00025);
        }

        assert!(!strategy.can_trade());

        strategy.track_candidate("NewMint".to_string(), "NEW".to_string());
        strategy
            .update_candidate("NewMint", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        let signal = strategy.evaluate_entry("NewMint");
        assert_eq!(signal, Some(LaunchSignal::Hold));
    }

    /// Test: Multiple entries track daily count
    #[test]
    fn test_entries_increment_daily_count() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        let stats_before = strategy.daily_stats();
        assert_eq!(stats_before.entries, 0);

        // Track and enter first position
        strategy.track_candidate("Mint1".to_string(), "TEST1".to_string());
        strategy
            .update_candidate("Mint1", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy
            .confirm_entry("Mint1", 0.001, 1_000_000, 50.0)
            .unwrap();

        let stats_after_1 = strategy.daily_stats();
        assert_eq!(stats_after_1.entries, 1);

        // Exit position
        strategy.confirm_exit("Mint1", 0.0015);

        // Enter second position
        strategy.track_candidate("Mint2".to_string(), "TEST2".to_string());
        strategy
            .update_candidate("Mint2", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy
            .confirm_entry("Mint2", 0.001, 1_000_000, 50.0)
            .unwrap();

        let stats_after_2 = strategy.daily_stats();
        assert_eq!(stats_after_2.entries, 2);
    }

    /// Test: Daily reset clears limits
    #[test]
    fn test_daily_reset() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        // Simulate some trading activity - track ALL candidates first
        for i in 0..5 {
            let mint = format!("Mint{}", i);
            strategy.track_candidate(mint.clone(), format!("TEST{}", i));
            strategy
                .update_candidate(&mint, 0.90, 0.001, 100, 0.02, 100.0)
                .unwrap();
        }

        for i in 0..5 {
            let mint = format!("Mint{}", i);
            strategy.confirm_entry(&mint, 0.001, 1_000_000, 50.0).unwrap();
            strategy.confirm_exit(&mint, 0.0008); // Small loss each time
        }

        let stats_before = strategy.daily_stats();
        assert!(stats_before.entries > 0);
        assert!(stats_before.pnl_usdc < 0.0);

        strategy.reset_daily();

        let stats_after = strategy.daily_stats();
        assert_eq!(stats_after.entries, 0);
        assert_eq!(stats_after.pnl_usdc, 0.0);
        assert!(strategy.can_trade());
    }

    /// Test: Daily stats tracking
    #[test]
    fn test_daily_stats() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        // Track ALL candidates first
        for i in 0..5 {
            let mint = format!("Mint{}", i);
            strategy.track_candidate(mint.clone(), format!("TEST{}", i));
            strategy
                .update_candidate(&mint, 0.90, 0.001, 100, 0.02, 100.0)
                .unwrap();
        }

        // 5 entries with profitable exits
        for i in 0..5 {
            let mint = format!("Mint{}", i);
            strategy.confirm_entry(&mint, 0.001, 1_000_000, 50.0).unwrap();
            strategy.confirm_exit(&mint, 0.0015); // 50% profit = +$25 each
        }

        let stats = strategy.daily_stats();
        assert_eq!(stats.entries, 5);
        assert_eq!(stats.max_entries, 10);
        assert!(stats.pnl_usdc > 100.0); // Should be ~$125
        assert_eq!(stats.max_loss_usdc, 100.0);
    }
}

// ============================================================================
// Test Module: Concurrent Position Limits
// ============================================================================

mod position_limits {
    use super::*;

    /// Test: Max concurrent positions enforced
    #[test]
    fn test_max_concurrent_positions() {
        let config = LaunchSniperConfig {
            max_concurrent_positions: 1,
            ..create_test_sniper_config()
        };
        let mut strategy = LaunchSniperStrategy::new(config);

        // Track two candidates
        strategy.track_candidate("Mint1".to_string(), "TEST1".to_string());
        strategy.track_candidate("Mint2".to_string(), "TEST2".to_string());

        // Set up both candidates as ready for entry
        strategy
            .update_candidate("Mint1", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy
            .update_candidate("Mint2", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Enter first position
        strategy
            .confirm_entry("Mint1", 0.001, 1_000_000, 50.0)
            .unwrap();

        // Second entry should be blocked
        let signal = strategy.evaluate_entry("Mint2");
        assert_eq!(signal, Some(LaunchSignal::Hold));
    }

    /// Test: After exit, new entry allowed
    #[test]
    fn test_entry_allowed_after_exit() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        strategy.track_candidate("Mint1".to_string(), "TEST1".to_string());
        strategy.track_candidate("Mint2".to_string(), "TEST2".to_string());

        strategy
            .update_candidate("Mint1", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy
            .update_candidate("Mint2", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Enter and exit first position
        strategy
            .confirm_entry("Mint1", 0.001, 1_000_000, 50.0)
            .unwrap();
        strategy.confirm_exit("Mint1", 0.0015);

        // Second entry should now be possible
        assert!(strategy.get_positions().is_empty());
        let signal = strategy.evaluate_entry("Mint2");
        // Signal depends on velocity checks, but should not be blocked by position limit
        assert!(signal.is_some());
    }
}

// ============================================================================
// Test Module: Cooldown Enforcement
// ============================================================================

mod cooldown_enforcement {
    use super::*;

    /// Test: Cooldown prevents rapid re-entry
    #[test]
    fn test_cooldown_prevents_rapid_entry() {
        let config = LaunchSniperConfig {
            cooldown_seconds: 60, // 60 second cooldown
            ..create_test_sniper_config()
        };
        let mut strategy = LaunchSniperStrategy::new(config);

        strategy.track_candidate("Mint1".to_string(), "TEST1".to_string());
        strategy.track_candidate("Mint2".to_string(), "TEST2".to_string());

        strategy
            .update_candidate("Mint1", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy
            .update_candidate("Mint2", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        // Enter and exit first position
        strategy
            .confirm_entry("Mint1", 0.001, 1_000_000, 50.0)
            .unwrap();
        strategy.confirm_exit("Mint1", 0.0015);

        // Immediately try to enter second position
        assert!(strategy.is_in_cooldown());
        let remaining = strategy.cooldown_remaining();
        assert!(remaining > 0);

        let signal = strategy.evaluate_entry("Mint2");
        assert_eq!(signal, Some(LaunchSignal::Hold)); // Blocked by cooldown
    }
}

// ============================================================================
// Test Module: Safety Check Integration
// ============================================================================

mod safety_check_integration {
    use super::*;

    /// Test: Candidate safety checks integrate with config thresholds
    #[test]
    fn test_candidate_safety_via_update() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // Update with values that should PASS safety checks
        strategy
            .update_candidate(
                "Mint123",
                0.90,  // bonding percent
                0.001, // price
                100,   // holder count - passes min of 50
                0.02,  // creator holding 2% - passes max of 5%
                100.0, // liquidity - passes min of 10
            )
            .unwrap();

        let candidate = strategy.get_candidate("Mint123").unwrap();
        assert!(candidate.passed_safety);

        // Now update with values that should FAIL
        strategy
            .update_candidate(
                "Mint123",
                0.90,
                0.001,
                100,
                0.10,  // creator holding 10% - EXCEEDS max of 5%
                100.0,
            )
            .unwrap();

        let candidate = strategy.get_candidate("Mint123").unwrap();
        assert!(!candidate.passed_safety);
    }

    /// Test: Combined rug detector + sniper safety flow
    #[test]
    fn test_combined_safety_flow() {
        let mut detector = RugDetector::pump_fun_sniper();
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());

        // First, run rug detector analysis - use values compatible with BOTH configs:
        // rug detector pump_fun_sniper: min_holder_count=10, max_single=10%, min_liquidity_usd=1000
        // sniper config: min_unique_holders=50, max_creator_holding_percent=0.05 (5%), min_liquidity_sol=10
        let token_data = create_mock_token_analysis_data("Mint123", false); // No authorities
        let holders = create_mock_holders(60, 3.0); // 60 holders, 3% max = passes both configs
        let liquidity = create_mock_liquidity(15_000.0, false); // 15k USD = ~100 SOL

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(report.is_safe(), "Report warnings: {:?}", report.warnings);

        // Then update sniper candidate based on rug report data
        let holder_count = holders.len() as u32; // 60 holders - passes min_unique_holders=50
        let creator_holding = holders
            .iter()
            .find(|h| h.is_creator)
            .map(|h| h.percentage / 100.0)
            .unwrap_or(0.0); // 3% / 100 = 0.03 - passes max_creator_holding_percent=0.05
        let liquidity_sol = liquidity.liquidity_usd / 150.0; // ~100 SOL - passes min_liquidity_sol=10

        strategy
            .update_candidate("Mint123", 0.90, 0.001, holder_count, creator_holding, liquidity_sol)
            .unwrap();

        let candidate = strategy.get_candidate("Mint123").unwrap();
        // Both should pass for safe token
        assert!(report.is_safe());
        assert!(candidate.passed_safety, "Sniper safety failure: {:?}", candidate.safety_failure_reason);
    }
}

// ============================================================================
// Test Module: Event Processing (Pump Types)
// ============================================================================

mod event_processing {
    use super::*;

    /// Test: PumpFunToken has correct fields
    #[test]
    fn test_pump_fun_token_fields() {
        let token = create_mock_pump_token("TestMint", "MEME", "CreatorAddr");

        assert_eq!(token.mint, "TestMint");
        assert_eq!(token.name, "MEME Token");
        assert_eq!(token.symbol, "MEME");
        assert_eq!(token.creator, "CreatorAddr");
        assert_eq!(token.initial_buy, 1_000_000);
        assert!((token.market_cap_sol - 25.0).abs() < 0.001);
        assert!(token.uri.is_some());
        assert!(token.has_socials());
    }

    /// Test: TradeInfo has correct fields
    #[test]
    fn test_trade_info_fields() {
        let trade = create_mock_trade("TradeMint", true, 2.5, 60_000_000_000);

        assert_eq!(trade.mint, "TradeMint");
        assert!(trade.is_buy);
        assert_eq!(trade.sol_amount, 2_500_000_000); // 2.5 SOL in lamports
        assert_eq!(trade.trader, "TraderWallet123");
        assert!(trade.market_cap_sol > 0.0);
        assert!(trade.is_whale_trade()); // > 1 SOL
    }

    /// Test: BondingCurveState calculations
    #[test]
    fn test_bonding_curve_calculations() {
        let curve = create_mock_bonding_curve("Mint123", 42_500_000_000, false);

        let progress = curve.graduation_progress();
        assert!((progress - 50.0).abs() < 0.1);
        assert!(!curve.is_near_graduation());

        let near_grad_curve = create_mock_bonding_curve("Mint123", 70_000_000_000, false);
        assert!(near_grad_curve.is_near_graduation());
    }

    /// Test: TradeInfo amount methods
    #[test]
    fn test_trade_amount_methods() {
        let small_trade = create_mock_trade("Mint", true, 0.05, 30_000_000_000);
        assert!(!small_trade.is_significant()); // < 0.1 SOL
        assert!(!small_trade.is_whale_trade()); // < 1 SOL

        let large_trade = create_mock_trade("Mint", true, 1.5, 35_000_000_000);
        assert!(large_trade.is_significant());
        assert!(large_trade.is_whale_trade());
    }
}

// ============================================================================
// Test Module: Position PnL Calculations
// ============================================================================

mod pnl_calculations {
    use super::*;

    /// Test: Position PnL percentage calculation via strategy
    #[test]
    fn test_position_pnl_via_strategy() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());
        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();

        // Exit with 50% gain
        let pnl = strategy.confirm_exit("Mint123", 0.0015);
        assert!(pnl.is_some());
        assert!((pnl.unwrap() - 50.0).abs() < 0.1);
    }

    /// Test: Exit with loss tracking
    #[test]
    fn test_exit_loss_tracking() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());
        strategy.track_candidate("Mint123".to_string(), "TEST".to_string());
        strategy
            .update_candidate("Mint123", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();

        strategy
            .confirm_entry("Mint123", 0.001, 1_000_000, 50.0)
            .unwrap();

        // Exit with 20% loss
        let pnl = strategy.confirm_exit("Mint123", 0.0008);
        assert!(pnl.is_some());
        assert!((pnl.unwrap() - (-20.0)).abs() < 0.1);

        let stats = strategy.daily_stats();
        assert!(stats.pnl_usdc < 0.0);
    }

    /// Test: Cumulative PnL tracking across multiple trades
    #[test]
    fn test_cumulative_pnl_tracking() {
        let mut strategy = LaunchSniperStrategy::new(create_test_sniper_config());

        // Trade 1: 50% profit on $50 = +$25
        strategy.track_candidate("Mint1".to_string(), "TEST1".to_string());
        strategy
            .update_candidate("Mint1", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy.confirm_entry("Mint1", 0.001, 1_000_000, 50.0).unwrap();
        strategy.confirm_exit("Mint1", 0.0015);

        // Trade 2: 20% loss on $50 = -$10
        strategy.track_candidate("Mint2".to_string(), "TEST2".to_string());
        strategy
            .update_candidate("Mint2", 0.90, 0.001, 100, 0.02, 100.0)
            .unwrap();
        strategy.confirm_entry("Mint2", 0.001, 1_000_000, 50.0).unwrap();
        strategy.confirm_exit("Mint2", 0.0008);

        // Net should be +$15
        let stats = strategy.daily_stats();
        assert!((stats.pnl_usdc - 15.0).abs() < 0.5);
    }
}

// ============================================================================
// Test Module: Risk Level Integration
// ============================================================================

mod risk_level_integration {
    use super::*;

    /// Test: Risk level position size multiplier
    #[test]
    fn test_risk_level_position_sizing() {
        assert_eq!(RiskLevel::Safe.position_size_multiplier(), 1.0);
        assert_eq!(RiskLevel::Low.position_size_multiplier(), 0.75);
        assert_eq!(RiskLevel::Medium.position_size_multiplier(), 0.25);
        assert_eq!(RiskLevel::High.position_size_multiplier(), 0.0);
        assert_eq!(RiskLevel::Critical.position_size_multiplier(), 0.0);
    }

    /// Test: Risk level should_block check
    #[test]
    fn test_risk_level_blocking() {
        assert!(!RiskLevel::Safe.should_block());
        assert!(!RiskLevel::Low.should_block());
        assert!(!RiskLevel::Medium.should_block());
        assert!(RiskLevel::High.should_block());
        assert!(RiskLevel::Critical.should_block());
    }

    /// Test: Multiple warnings accumulate risk
    #[test]
    fn test_cumulative_risk_scoring() {
        let mut report = TokenSafetyReport::new("Test".to_string());

        report.add_warning(RugWarning::LowLiquidity {
            liquidity_usd: 500.0,
            minimum_usd: 10_000.0,
        });
        report.calculate_risk();
        let score1 = report.risk_score;

        report.add_warning(RugWarning::LowHolderCount {
            count: 20,
            minimum: 100,
        });
        report.calculate_risk();
        let score2 = report.risk_score;

        assert!(score2 > score1);
    }

    /// Test: Risk level determines tradeability
    #[test]
    fn test_risk_level_tradeability() {
        assert!(RiskLevel::Safe.is_tradeable());
        assert!(RiskLevel::Low.is_tradeable());
        assert!(!RiskLevel::Medium.is_tradeable());
        assert!(!RiskLevel::High.is_tradeable());
        assert!(!RiskLevel::Critical.is_tradeable());
    }
}
