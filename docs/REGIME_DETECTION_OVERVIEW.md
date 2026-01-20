# Market Regime Detection Overview

## Why Regime Detection?

Mean reversion strategies fail in trending markets. Regime detection identifies when to trade and when to sit out.

## Architecture

### Trait-Based Design

```rust
/// Any regime detector implements this trait
pub trait RegimeDetector: Send + Sync {
    fn update(&mut self, candle: &Candle) -> Option<RegimeSignal>;
    fn get_position_multiplier(&self) -> f64;
    fn is_ready(&self) -> bool;
    fn name(&self) -> &'static str;
}

/// Signal with confidence for ensemble blending
pub enum RegimeSignal {
    Favorable(f64),    // 0.8-1.0 confidence
    Neutral(f64),      // 0.3-0.7 confidence
    Unfavorable(f64),  // 0.0-0.2 confidence
}

impl RegimeSignal {
    pub fn confidence(&self) -> f64 {
        match self {
            Self::Favorable(c) | Self::Neutral(c) | Self::Unfavorable(c) => *c
        }
    }

    pub fn from_confidence(c: f64) -> Self {
        if c >= 0.7 { Self::Favorable(c) }
        else if c >= 0.4 { Self::Neutral(c) }
        else { Self::Unfavorable(c) }
    }
}
```

### Implementations

| Detector | Struct | Purpose |
|----------|--------|---------|
| ADX | `AdxRegimeDetector` | Trend strength |
| Hurst | `HurstRegimeDetector` | Mean-reversion probability |
| BB Width | `BollingerWidthDetector` | Squeeze/expansion |
| Ensemble | `EnsembleDetector` | Weighted blend |

### Parallel Execution Model

All detectors process the same candle stream independently (parallel, not serial):

```rust
impl EnsembleDetector {
    fn update(&mut self, candle: &Candle) -> Option<RegimeSignal> {
        // Each detector processes same candle in parallel
        let signals: Vec<_> = self.detectors.iter_mut()
            .filter_map(|(d, w)| d.update(candle).map(|s| (s, *w)))
            .collect();

        // Weighted average of confidence values
        let weighted_sum: f64 = signals.iter()
            .map(|(s, w)| s.confidence() * w)
            .sum();
        let total_weight: f64 = signals.iter().map(|(_, w)| w).sum();

        let avg = weighted_sum / total_weight;
        Some(RegimeSignal::from_confidence(avg))
    }
}
```

**Why parallel?**
- ADX, Hurst, BB are independent - no cross-detector dependencies
- Cleaner mental model (pure functions of price history)
- Fits async runtime naturally

## Detection Methods

### 1. ADX (Average Directional Index)
- **Complexity**: Low
- **Best for**: Trend strength filtering
- **Threshold**: ADX < 20-25 for mean reversion
- **Recalc**: Every candle

### 2. ATR Ratio (Volatility Clustering)
- **Complexity**: Low
- **Best for**: Volatility regime shifts
- **Formula**: `ATR(7) / ATR(50)`
- **Ranges**: < 0.8 = compression, 0.8-1.5 = normal, > 1.5 = expansion

### 3. Bollinger Band Width
- **Complexity**: Low
- **Best for**: Squeeze/expansion detection
- **Formula**: `(Upper - Lower) / Middle * 100`
- **Signal**: Width at 6-month low = breakout imminent

### 4. Hurst Exponent
- **Complexity**: Medium
- **Best for**: Strategy selection
- **Interpretation**:
  - H < 0.45: Mean-reverting (trade it!)
  - H ≈ 0.5: Random walk (no edge)
  - H > 0.55: Trending (avoid mean reversion)
- **Recalc**: Every 4 hours

### 5. Hidden Markov Models (HMM)
- **Complexity**: High
- **Best for**: Multi-state regime classification
- **Performance**: Sharpe 1.9 in research
- **Recalc**: Every 1-4 hours

## Ensemble Approach

Combine multiple signals with weighted voting:

```python
weights = {
    'adx': 0.20,
    'atr_ratio': 0.20,
    'bb_width': 0.15,
    'hurst': 0.25,
    'hmm': 0.20
}

# Score each indicator 0-1 for mean-reversion favorability
# Weighted sum > 0.7 = favorable, 0.4-0.7 = neutral, < 0.4 = unfavorable
```

## State Machine Design

```
States:
  MEAN_REVERTING -> Full trading (1.0x size)
  TRENDING -> No trading (0.0x size)
  HIGH_VOLATILITY -> Reduced trading (0.5x size)
  SQUEEZE -> No trading (breakout imminent)
  UNKNOWN -> No trading (wait for clarity)

Transitions:
  - Require minimum time in current state (5-10 min)
  - Use hysteresis (different entry/exit thresholds)
  - Require confirmation (3+ bars above threshold)
```

## Implementation Priority

1. **Must-have**: ADX + ATR Ratio (low complexity, high impact)
2. **Recommended**: Hurst Exponent (medium complexity, high value)
3. **Nice-to-have**: BB Width (easy addition)
4. **Advanced**: HMM (high complexity, highest accuracy)

## Expected Results

| Filter | Drawdown Reduction | False Signal Reduction |
|--------|-------------------|----------------------|
| ADX only | ~20-30% | ~25% |
| ADX + ATR | ~35-40% | ~35% |
| Full ensemble | ~50%+ | ~40-50% |

## Quick Start Config

```toml
[regime_detection]
enabled = true
adx_period = 14
adx_threshold = 20
atr_short_period = 7
atr_long_period = 50
min_regime_duration_seconds = 300
enable_hurst = false  # Add later

# Ensemble weights (must sum to 1.0)
[regime_detection.weights]
adx = 0.5
atr_ratio = 0.3
bb_width = 0.2
```

## File Structure

```
src/strategy/
├── regime/
│   ├── mod.rs              # RegimeDetector trait + RegimeSignal enum
│   ├── adx.rs              # AdxRegimeDetector
│   ├── atr_ratio.rs        # AtrRatioDetector
│   ├── bollinger_width.rs  # BollingerWidthDetector
│   ├── hurst.rs            # HurstRegimeDetector (future)
│   └── ensemble.rs         # EnsembleDetector
```
