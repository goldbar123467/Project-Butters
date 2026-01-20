# ADX Regime Detection for Mean Reversion

## Overview

ADX (Average Directional Index) measures trend strength on a 0-100 scale. Lower values indicate ranging conditions ideal for mean reversion strategies.

## Trait Implementation

```rust
use crate::strategy::regime::{RegimeDetector, RegimeSignal, Candle};

pub struct AdxRegimeDetector {
    period: usize,
    entry_threshold: f64,  // 20.0 - enable trading below
    exit_threshold: f64,   // 28.0 - disable trading above

    // Wilder's smoothed values
    smoothed_plus_dm: f64,
    smoothed_minus_dm: f64,
    smoothed_tr: f64,
    smoothed_adx: f64,

    // Previous candle
    prev_high: f64,
    prev_low: f64,
    prev_close: f64,

    // State
    count: usize,
    is_trading_enabled: bool,
}

impl RegimeDetector for AdxRegimeDetector {
    fn update(&mut self, candle: &Candle) -> Option<RegimeSignal> {
        let adx = self.calculate_adx(candle)?;
        self.update_regime_state(adx);

        // Convert ADX to confidence (inverse relationship)
        let confidence = self.adx_to_confidence(adx);
        Some(RegimeSignal::from_confidence(confidence))
    }

    fn get_position_multiplier(&self) -> f64 {
        // Based on last calculated ADX
        self.calculate_multiplier()
    }

    fn is_ready(&self) -> bool {
        self.count >= 2 * self.period - 1
    }

    fn name(&self) -> &'static str {
        "ADX"
    }
}
```

## Core Formula

```
True Range (TR) = max(High - Low, |High - Prev_Close|, |Prev_Close - Low|)

+DM = Current_High - Previous_High (if positive and > down move)
-DM = Previous_Low - Current_Low (if positive and > up move)

Wilder's Smoothing: Smoothed = Prev_Smoothed * (n-1)/n + Current_Value

+DI = (Smoothed_+DM / Smoothed_TR) * 100
-DI = (Smoothed_-DM / Smoothed_TR) * 100

DX = |+DI - -DI| / (+DI + -DI) * 100
ADX = (Previous_ADX * (n-1) + Current_DX) / n
```

## Regime Thresholds for Crypto

| ADX Value | Market Regime | Mean Reversion Action |
|-----------|---------------|----------------------|
| < 15 | Very weak/no trend | Full position size |
| 15-20 | Weak trend | 80% position size |
| 20-25 | Moderate trend | 50% position size |
| 25-30 | Strong trend | 20% position size |
| > 30 | Very strong trend | **No trading** |

## Implementation with Hysteresis

Prevent whipsaw by using different entry/exit thresholds:

```rust
const ENTRY_THRESHOLD: f64 = 20.0;  // Enable trading when ADX drops below
const EXIT_THRESHOLD: f64 = 28.0;   // Disable trading when ADX rises above

fn update_regime(&mut self, adx: f64) {
    if self.is_trading_enabled {
        if adx > EXIT_THRESHOLD {
            self.is_trading_enabled = false;
        }
    } else {
        if adx < ENTRY_THRESHOLD {
            self.is_trading_enabled = true;
        }
    }
}
```

## Position Size Scaling

```rust
fn calculate_position_multiplier(adx: f64) -> f64 {
    if adx < 15.0 { 1.0 }
    else if adx < 20.0 { 0.8 }
    else if adx < 25.0 { 0.5 }
    else if adx < 30.0 { 0.2 }
    else { 0.0 }
}
```

## Optimal Parameters for SOL/Crypto

| Parameter | Recommended Value |
|-----------|------------------|
| ADX Period | 10-14 (shorter for responsiveness) |
| Max Threshold | 20-25 (conservative) |
| Confirmation Periods | 3 bars minimum |
| Warmup Period | 2 * period bars |

## Complementary Indicators

1. **ATR Ratio**: `ATR(7) / ATR(50)` - volatility regime
2. **Bollinger Band Width**: Squeeze detection
3. **Hurst Exponent**: H < 0.45 = mean-reverting

## Rust Implementation

Recommended crate: **yata**

```toml
[dependencies]
yata = { version = "0.7", features = ["serde"] }
```

```rust
use yata::prelude::*;
use yata::indicators::AverageDirectionalIndex;

let mut adx = AverageDirectionalIndex::default();
let mut state = adx.init(&first_candle).unwrap();

// Streaming updates
let result = state.next(&candle);
let adx_value = result.values()[0] * 100.0; // yata returns 0-1, scale to 0-100
```

## Expected Impact

- ADX filter alone: ~37% improvement in profitability
- Reduces drawdowns during trending markets
- Prevents trading against strong momentum

## Confidence Mapping

ADX to confidence (for ensemble blending):

```rust
fn adx_to_confidence(&self, adx: f64) -> f64 {
    // Inverse mapping: low ADX = high confidence for mean reversion
    if adx < 15.0 { 0.95 }
    else if adx < 20.0 { 0.80 }
    else if adx < 25.0 { 0.50 }
    else if adx < 30.0 { 0.25 }
    else { 0.10 }
}
```

This maps to RegimeSignal:
- ADX < 20 → `Favorable(0.80-0.95)`
- ADX 20-25 → `Neutral(0.50)`
- ADX > 25 → `Unfavorable(0.10-0.25)`

## Integration with Ensemble

```rust
let ensemble = EnsembleDetector::new(vec![
    (Box::new(AdxRegimeDetector::new(14)), 0.5),   // 50% weight
    (Box::new(AtrRatioDetector::new(7, 50)), 0.3), // 30% weight
    (Box::new(BollingerWidthDetector::new(20)), 0.2), // 20% weight
]);

// In orchestrator
if let Some(signal) = ensemble.update(&candle) {
    let multiplier = signal.confidence();
    position_size = base_size * multiplier;
}
```
