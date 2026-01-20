# ADX (Average Directional Index) Technical Specification

## Overview

The Average Directional Index (ADX) was developed by J. Welles Wilder in 1978 and published in his book "New Concepts in Technical Trading Systems." ADX measures trend strength without indicating trend direction, ranging from 0 to 100.

The ADX system consists of three components:
- **+DI (Plus Directional Indicator)**: Measures upward price movement strength
- **-DI (Minus Directional Indicator)**: Measures downward price movement strength
- **ADX**: Smoothed average of the Directional Index (DX), measuring overall trend strength

---

## Mathematical Formulas

### Step 1: True Range (TR)

True Range accounts for gaps between sessions:

```
TR = max(
    High - Low,
    |High - Previous_Close|,
    |Previous_Close - Low|
)
```

**Edge Cases:**
- First bar: TR = High - Low (no previous close available)
- If High == Low (doji): TR = max(0, |High - Prev_Close|, |Prev_Close - Low|)

### Step 2: Directional Movement (+DM and -DM)

Calculate the directional movement for each bar:

```
Up_Move   = Current_High - Previous_High
Down_Move = Previous_Low - Current_Low

if Up_Move > Down_Move AND Up_Move > 0:
    +DM = Up_Move
    -DM = 0
elif Down_Move > Up_Move AND Down_Move > 0:
    +DM = 0
    -DM = Down_Move
else:
    +DM = 0
    -DM = 0
```

**Important Rules:**
1. Only ONE of +DM or -DM can be non-zero per bar (the larger movement wins)
2. Both are set to 0 if movements are equal
3. Both are set to 0 if the dominant movement is negative or zero
4. Inside bars (where current range is within previous range) have +DM = -DM = 0

### Step 3: Wilder's Smoothing

Wilder's smoothing is a modified exponential moving average where:
- α (smoothing factor) = 1/n (vs. 2/(n+1) for standard EMA)
- Equivalent to a (2n-1) period standard EMA

**First Value (Initialization):**
```
Smoothed_TR[n-1]  = sum(TR[0..n])
Smoothed_+DM[n-1] = sum(+DM[0..n])
Smoothed_-DM[n-1] = sum(-DM[0..n])
```

**Subsequent Values:**
```
Smoothed_TR[i]  = Smoothed_TR[i-1]  - (Smoothed_TR[i-1] / n)  + TR[i]
Smoothed_+DM[i] = Smoothed_+DM[i-1] - (Smoothed_+DM[i-1] / n) + +DM[i]
Smoothed_-DM[i] = Smoothed_-DM[i-1] - (Smoothed_-DM[i-1] / n) + -DM[i]
```

**Alternative Form (Recursive):**
```
Smoothed[i] = ((n - 1) / n) * Smoothed[i-1] + (1 / n) * Current_Value
            = Smoothed[i-1] * (n-1)/n + Current_Value / n
```

### Step 4: Directional Indicators (+DI and -DI)

```
+DI = (Smoothed_+DM / Smoothed_TR) * 100
-DI = (Smoothed_-DM / Smoothed_TR) * 100
```

**Edge Case:** If Smoothed_TR == 0, set both +DI and -DI to 0

### Step 5: Directional Index (DX)

```
DX = |+DI - (-DI)| / (+DI + (-DI)) * 100
   = abs(+DI - -DI) / (+DI + -DI) * 100
```

**Edge Case:** If (+DI + -DI) == 0, set DX to 0

### Step 6: ADX (Average Directional Index)

**First ADX Value:**
```
ADX[2n-2] = mean(DX[n-1..2n-1])  // Average of first n DX values
```

**Subsequent ADX Values (Wilder's Smoothing):**
```
ADX[i] = (ADX[i-1] * (n - 1) + DX[i]) / n
```

---

## Lookback Period Analysis

### Standard Period: 14

Wilder's original recommendation. Provides a good balance between:
- Responsiveness to trend changes
- Filtering out noise/false signals

### Period Comparison for Crypto

| Period | Use Case | Pros | Cons |
|--------|----------|------|------|
| 7 | Day trading, scalping | Fast signals, catches early trends | More noise, false signals |
| 10 | Active trading | Good for 15m-1H timeframes | Moderate noise |
| **14** | Standard/Default | Balanced, well-tested | May lag on very volatile crypto |
| 20 | Swing trading | Smoother, fewer false signals | Misses early trend signals |
| 28 | Position trading | Best for daily+ timeframes | Significant lag |

### Crypto-Specific Recommendations

For cryptocurrency markets (higher volatility than traditional markets):

1. **Intraday (5m-15m)**: Period 7-10, but expect more noise
2. **Short-term (1H-4H)**: Period 14 (standard)
3. **Medium-term (Daily)**: Period 14-20
4. **Long-term (Weekly)**: Period 20-28

**Note:** The effective smoothing period is (2n-1). So:
- 14-period Wilder's = 27-period EMA equivalent
- 20-period Wilder's = 39-period EMA equivalent

---

## Threshold Values for Regime Detection

### Traditional Interpretation

| ADX Range | Trend Strength | Market Regime |
|-----------|----------------|---------------|
| 0-20 | Absent/Weak | Ranging/Consolidating |
| 20-25 | Neutral Zone | Trend emerging or fading |
| 25-40 | Strong | Trending |
| 40-50 | Very Strong | Strong trend |
| 50-75 | Extremely Strong | Powerful trend |
| 75-100 | Extremely Strong | Rare, often near exhaustion |

### Crypto-Specific Thresholds

Due to higher volatility in crypto markets:

| ADX Range | Crypto Interpretation |
|-----------|----------------------|
| 0-20 | Weak trend, range-bound (use mean-reversion strategies) |
| 20-25 | Transition zone (wait for confirmation) |
| **25-30** | Trend emerging (start considering trend trades) |
| 30-40 | Confirmed trend (optimal for trend-following) |
| **40+** | Strong trend (but watch for exhaustion) |
| 50+ | Very strong, often late-stage (reduce position size) |

### Recommended Thresholds for Implementation

```rust
const ADX_RANGING_THRESHOLD: f64 = 20.0;      // Below = ranging market
const ADX_TRENDING_THRESHOLD: f64 = 25.0;     // Above = trending market
const ADX_STRONG_TREND_THRESHOLD: f64 = 40.0; // Above = strong trend
const ADX_EXHAUSTION_WARNING: f64 = 50.0;     // Above = potential exhaustion

// For crypto-specific (more conservative)
const CRYPTO_ADX_TRENDING_THRESHOLD: f64 = 30.0;
const CRYPTO_ADX_STRONG_THRESHOLD: f64 = 45.0;
```

### DI Crossover Rules

Trend direction is determined by +DI vs -DI relationship:
- **Bullish**: +DI > -DI
- **Bearish**: -DI > +DI

**Important:** Only consider DI crossovers valid when ADX > 25

---

## Efficient Rust Implementation

### Data Structures

```rust
/// Configuration for ADX calculation
#[derive(Debug, Clone)]
pub struct AdxConfig {
    /// Lookback period (default: 14)
    pub period: usize,
    /// Minimum periods before first ADX value
    pub warmup_periods: usize,
}

impl Default for AdxConfig {
    fn default() -> Self {
        Self {
            period: 14,
            warmup_periods: 27, // period + (period - 1) for DX smoothing
        }
    }
}

/// ADX indicator state for incremental calculation
#[derive(Debug, Clone)]
pub struct AdxState {
    config: AdxConfig,

    // Previous bar data
    prev_high: Option<f64>,
    prev_low: Option<f64>,
    prev_close: Option<f64>,

    // Smoothed values (Wilder's smoothing)
    smoothed_tr: f64,
    smoothed_plus_dm: f64,
    smoothed_minus_dm: f64,

    // ADX smoothing
    adx: f64,

    // Initialization tracking
    bars_processed: usize,

    // Initialization accumulators
    tr_sum: f64,
    plus_dm_sum: f64,
    minus_dm_sum: f64,
    dx_values: Vec<f64>,

    // Latest output values
    pub plus_di: f64,
    pub minus_di: f64,
    pub adx_value: f64,
}

/// Single bar OHLC data
#[derive(Debug, Clone, Copy)]
pub struct OhlcBar {
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// ADX calculation result
#[derive(Debug, Clone, Copy)]
pub struct AdxResult {
    pub plus_di: f64,
    pub minus_di: f64,
    pub adx: f64,
    pub dx: f64,
    pub is_valid: bool,  // True once warmup complete
}
```

### Core Implementation

```rust
impl AdxState {
    pub fn new(config: AdxConfig) -> Self {
        Self {
            config: config.clone(),
            prev_high: None,
            prev_low: None,
            prev_close: None,
            smoothed_tr: 0.0,
            smoothed_plus_dm: 0.0,
            smoothed_minus_dm: 0.0,
            adx: 0.0,
            bars_processed: 0,
            tr_sum: 0.0,
            plus_dm_sum: 0.0,
            minus_dm_sum: 0.0,
            dx_values: Vec::with_capacity(config.period),
            plus_di: 0.0,
            minus_di: 0.0,
            adx_value: 0.0,
        }
    }

    /// Update with new bar data, returns ADX result
    pub fn update(&mut self, bar: &OhlcBar) -> AdxResult {
        let n = self.config.period as f64;

        // Calculate True Range
        let tr = self.calculate_true_range(bar);

        // Calculate Directional Movement
        let (plus_dm, minus_dm) = self.calculate_directional_movement(bar);

        // Store previous values for next iteration
        self.prev_high = Some(bar.high);
        self.prev_low = Some(bar.low);
        self.prev_close = Some(bar.close);

        self.bars_processed += 1;

        // Phase 1: Accumulate for first smoothed values (bars 1 to n)
        if self.bars_processed <= self.config.period {
            self.tr_sum += tr;
            self.plus_dm_sum += plus_dm;
            self.minus_dm_sum += minus_dm;

            // Initialize smoothed values at period n
            if self.bars_processed == self.config.period {
                self.smoothed_tr = self.tr_sum;
                self.smoothed_plus_dm = self.plus_dm_sum;
                self.smoothed_minus_dm = self.minus_dm_sum;

                // Calculate first DI values
                self.calculate_di_values();

                // Calculate first DX
                let dx = self.calculate_dx();
                self.dx_values.push(dx);
            }

            return AdxResult {
                plus_di: self.plus_di,
                minus_di: self.minus_di,
                adx: 0.0,
                dx: 0.0,
                is_valid: false,
            };
        }

        // Phase 2: Continue with Wilder's smoothing
        // Smoothed = Previous - (Previous / n) + Current
        self.smoothed_tr = self.smoothed_tr - (self.smoothed_tr / n) + tr;
        self.smoothed_plus_dm = self.smoothed_plus_dm - (self.smoothed_plus_dm / n) + plus_dm;
        self.smoothed_minus_dm = self.smoothed_minus_dm - (self.smoothed_minus_dm / n) + minus_dm;

        // Calculate DI values
        self.calculate_di_values();

        // Calculate DX
        let dx = self.calculate_dx();

        // Phase 2a: Accumulate DX for first ADX (bars n+1 to 2n-1)
        if self.bars_processed < 2 * self.config.period {
            self.dx_values.push(dx);

            // Initialize ADX at bar 2n-1
            if self.bars_processed == 2 * self.config.period - 1 {
                self.adx = self.dx_values.iter().sum::<f64>() / n;
                self.adx_value = self.adx;
            }

            return AdxResult {
                plus_di: self.plus_di,
                minus_di: self.minus_di,
                adx: self.adx_value,
                dx,
                is_valid: self.bars_processed >= 2 * self.config.period - 1,
            };
        }

        // Phase 3: Ongoing ADX calculation with Wilder's smoothing
        // ADX = (Previous_ADX * (n-1) + Current_DX) / n
        self.adx = (self.adx * (n - 1.0) + dx) / n;
        self.adx_value = self.adx;

        AdxResult {
            plus_di: self.plus_di,
            minus_di: self.minus_di,
            adx: self.adx_value,
            dx,
            is_valid: true,
        }
    }

    #[inline]
    fn calculate_true_range(&self, bar: &OhlcBar) -> f64 {
        match self.prev_close {
            Some(prev_close) => {
                let hl = bar.high - bar.low;
                let hc = (bar.high - prev_close).abs();
                let lc = (bar.low - prev_close).abs();
                hl.max(hc).max(lc)
            }
            None => bar.high - bar.low,
        }
    }

    #[inline]
    fn calculate_directional_movement(&self, bar: &OhlcBar) -> (f64, f64) {
        match (self.prev_high, self.prev_low) {
            (Some(prev_high), Some(prev_low)) => {
                let up_move = bar.high - prev_high;
                let down_move = prev_low - bar.low;

                if up_move > down_move && up_move > 0.0 {
                    (up_move, 0.0)
                } else if down_move > up_move && down_move > 0.0 {
                    (0.0, down_move)
                } else {
                    (0.0, 0.0)
                }
            }
            _ => (0.0, 0.0),
        }
    }

    #[inline]
    fn calculate_di_values(&mut self) {
        if self.smoothed_tr > 0.0 {
            self.plus_di = (self.smoothed_plus_dm / self.smoothed_tr) * 100.0;
            self.minus_di = (self.smoothed_minus_dm / self.smoothed_tr) * 100.0;
        } else {
            self.plus_di = 0.0;
            self.minus_di = 0.0;
        }
    }

    #[inline]
    fn calculate_dx(&self) -> f64 {
        let di_sum = self.plus_di + self.minus_di;
        if di_sum > 0.0 {
            ((self.plus_di - self.minus_di).abs() / di_sum) * 100.0
        } else {
            0.0
        }
    }

    /// Get current trend regime based on ADX value
    pub fn get_regime(&self) -> TrendRegime {
        if !self.is_valid() {
            return TrendRegime::Unknown;
        }

        if self.adx_value < 20.0 {
            TrendRegime::Ranging
        } else if self.adx_value < 25.0 {
            TrendRegime::Transitioning
        } else if self.adx_value < 40.0 {
            TrendRegime::Trending
        } else if self.adx_value < 50.0 {
            TrendRegime::StrongTrend
        } else {
            TrendRegime::ExtremeTrend
        }
    }

    /// Get trend direction based on DI crossover
    pub fn get_direction(&self) -> TrendDirection {
        if self.plus_di > self.minus_di {
            TrendDirection::Bullish
        } else if self.minus_di > self.plus_di {
            TrendDirection::Bearish
        } else {
            TrendDirection::Neutral
        }
    }

    /// Check if ADX is valid (warmup complete)
    pub fn is_valid(&self) -> bool {
        self.bars_processed >= 2 * self.config.period - 1
    }

    /// Reset state for reuse
    pub fn reset(&mut self) {
        self.prev_high = None;
        self.prev_low = None;
        self.prev_close = None;
        self.smoothed_tr = 0.0;
        self.smoothed_plus_dm = 0.0;
        self.smoothed_minus_dm = 0.0;
        self.adx = 0.0;
        self.bars_processed = 0;
        self.tr_sum = 0.0;
        self.plus_dm_sum = 0.0;
        self.minus_dm_sum = 0.0;
        self.dx_values.clear();
        self.plus_di = 0.0;
        self.minus_di = 0.0;
        self.adx_value = 0.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendRegime {
    Unknown,      // Not enough data
    Ranging,      // ADX < 20
    Transitioning, // 20 <= ADX < 25
    Trending,     // 25 <= ADX < 40
    StrongTrend,  // 40 <= ADX < 50
    ExtremeTrend, // ADX >= 50
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    Bullish,   // +DI > -DI
    Bearish,   // -DI > +DI
    Neutral,   // +DI == -DI
}
```

### Batch Calculation (Vectorized)

```rust
/// Calculate ADX for a complete price series (more efficient for backtesting)
pub fn calculate_adx_batch(
    highs: &[f64],
    lows: &[f64],
    closes: &[f64],
    period: usize,
) -> Result<AdxBatchResult, AdxError> {
    let len = highs.len();

    if len != lows.len() || len != closes.len() {
        return Err(AdxError::LengthMismatch);
    }

    if len < 2 * period {
        return Err(AdxError::InsufficientData);
    }

    let n = period as f64;

    // Pre-allocate output vectors
    let mut tr_values = Vec::with_capacity(len);
    let mut plus_dm_values = Vec::with_capacity(len);
    let mut minus_dm_values = Vec::with_capacity(len);
    let mut plus_di = vec![0.0; len];
    let mut minus_di = vec![0.0; len];
    let mut adx = vec![0.0; len];

    // Step 1: Calculate TR, +DM, -DM for all bars
    tr_values.push(highs[0] - lows[0]);
    plus_dm_values.push(0.0);
    minus_dm_values.push(0.0);

    for i in 1..len {
        // True Range
        let tr = (highs[i] - lows[i])
            .max((highs[i] - closes[i - 1]).abs())
            .max((lows[i] - closes[i - 1]).abs());
        tr_values.push(tr);

        // Directional Movement
        let up_move = highs[i] - highs[i - 1];
        let down_move = lows[i - 1] - lows[i];

        if up_move > down_move && up_move > 0.0 {
            plus_dm_values.push(up_move);
            minus_dm_values.push(0.0);
        } else if down_move > up_move && down_move > 0.0 {
            plus_dm_values.push(0.0);
            minus_dm_values.push(down_move);
        } else {
            plus_dm_values.push(0.0);
            minus_dm_values.push(0.0);
        }
    }

    // Step 2: Apply Wilder's smoothing
    let mut smoothed_tr: f64 = tr_values[..period].iter().sum();
    let mut smoothed_plus_dm: f64 = plus_dm_values[..period].iter().sum();
    let mut smoothed_minus_dm: f64 = minus_dm_values[..period].iter().sum();

    // First DI values
    let first_di_idx = period - 1;
    if smoothed_tr > 0.0 {
        plus_di[first_di_idx] = (smoothed_plus_dm / smoothed_tr) * 100.0;
        minus_di[first_di_idx] = (smoothed_minus_dm / smoothed_tr) * 100.0;
    }

    // Calculate DX values for ADX initialization
    let mut dx_values = Vec::with_capacity(period);
    let di_sum = plus_di[first_di_idx] + minus_di[first_di_idx];
    if di_sum > 0.0 {
        dx_values.push(((plus_di[first_di_idx] - minus_di[first_di_idx]).abs() / di_sum) * 100.0);
    } else {
        dx_values.push(0.0);
    }

    // Continue smoothing for remaining bars
    for i in period..len {
        smoothed_tr = smoothed_tr - (smoothed_tr / n) + tr_values[i];
        smoothed_plus_dm = smoothed_plus_dm - (smoothed_plus_dm / n) + plus_dm_values[i];
        smoothed_minus_dm = smoothed_minus_dm - (smoothed_minus_dm / n) + minus_dm_values[i];

        if smoothed_tr > 0.0 {
            plus_di[i] = (smoothed_plus_dm / smoothed_tr) * 100.0;
            minus_di[i] = (smoothed_minus_dm / smoothed_tr) * 100.0;
        }

        // Calculate DX
        let di_sum = plus_di[i] + minus_di[i];
        let dx = if di_sum > 0.0 {
            ((plus_di[i] - minus_di[i]).abs() / di_sum) * 100.0
        } else {
            0.0
        };

        // ADX calculation
        if i < 2 * period - 1 {
            dx_values.push(dx);
        } else if i == 2 * period - 1 {
            // First ADX value
            dx_values.push(dx);
            adx[i] = dx_values.iter().sum::<f64>() / n;
        } else {
            // Subsequent ADX with Wilder's smoothing
            adx[i] = (adx[i - 1] * (n - 1.0) + dx) / n;
        }
    }

    Ok(AdxBatchResult {
        plus_di,
        minus_di,
        adx,
        first_valid_index: 2 * period - 1,
    })
}

#[derive(Debug)]
pub struct AdxBatchResult {
    pub plus_di: Vec<f64>,
    pub minus_di: Vec<f64>,
    pub adx: Vec<f64>,
    pub first_valid_index: usize,
}

#[derive(Debug)]
pub enum AdxError {
    LengthMismatch,
    InsufficientData,
}
```

---

## Common Pitfalls and Edge Cases

### 1. Initialization Issues

**Problem:** ADX requires significant warmup period (2n-1 bars minimum).

**Solution:**
- Track initialization state explicitly
- Return invalid/NaN until warmup complete
- For 14-period ADX, need 27 bars minimum

### 2. Zero Division

**Locations where division by zero can occur:**
- `+DI = Smoothed_+DM / Smoothed_TR` (TR can be 0 on flat bars)
- `DX = |+DI - -DI| / (+DI + -DI)` (sum can be 0)

**Solution:** Always check denominator and return 0 when denominator is 0

```rust
let result = if denominator > 0.0 {
    numerator / denominator
} else {
    0.0
};
```

### 3. Inside Bars

When current bar's range is entirely within previous bar's range:
- Both +DM and -DM will be 0
- This is correct behavior, not a bug

### 4. Gap Handling

**Problem:** Large gaps can cause extreme TR values.

**Solution:** TR formula automatically handles gaps via the |High - Prev_Close| and |Prev_Close - Low| components.

### 5. Floating Point Precision

**Problem:** Accumulated smoothing can drift due to floating point errors.

**Solution:**
- Use f64 for all calculations
- Consider periodic re-initialization for very long-running calculations
- For critical applications, use fixed-point or decimal arithmetic

### 6. ADX Rising vs Price Direction

**Critical Misconception:** Rising ADX does NOT indicate bullish price action!

- ADX measures trend strength, not direction
- ADX rises during strong downtrends
- Use +DI/-DI crossovers for direction

### 7. High ADX Warning

**Pitfall:** ADX > 50 often indicates trend exhaustion, not strength to continue.

**Solution:** Consider reducing position size when ADX is very high, as reversals become more likely.

### 8. Whipsaw in Transition Zones

**Problem:** ADX oscillating around 20-25 causes frequent regime changes.

**Solution:**
- Add hysteresis (e.g., require ADX > 27 to enter trending, ADX < 23 to exit)
- Use additional confirmation indicators

```rust
// Hysteresis example
const TREND_ENTRY_THRESHOLD: f64 = 27.0;
const TREND_EXIT_THRESHOLD: f64 = 23.0;

fn update_regime(&mut self, adx: f64) {
    self.regime = match self.regime {
        TrendRegime::Ranging => {
            if adx > TREND_ENTRY_THRESHOLD {
                TrendRegime::Trending
            } else {
                TrendRegime::Ranging
            }
        }
        TrendRegime::Trending => {
            if adx < TREND_EXIT_THRESHOLD {
                TrendRegime::Ranging
            } else {
                TrendRegime::Trending
            }
        }
        _ => self.regime,
    };
}
```

### 9. Period Selection Edge Cases

**For very short periods (< 7):**
- Extremely noisy
- Many false signals
- Not recommended for most use cases

**For very long periods (> 30):**
- Very slow to react
- May miss entire trends
- High lag makes it less useful

### 10. Data Quality Issues

**Problem:** Missing data, extreme outliers, or incorrect OHLC relationships.

**Solution:**
```rust
fn validate_bar(bar: &OhlcBar) -> bool {
    bar.high >= bar.low &&
    bar.close >= bar.low &&
    bar.close <= bar.high &&
    bar.high.is_finite() &&
    bar.low.is_finite() &&
    bar.close.is_finite()
}
```

---

## Performance Optimization Tips

### 1. Avoid Repeated Allocations

Pre-allocate vectors in batch calculations.

### 2. Use SIMD Where Possible

TR calculation involves max operations that can be vectorized.

### 3. Incremental vs Batch

- **Incremental:** Better for live trading (constant memory, O(1) per update)
- **Batch:** Better for backtesting (can use SIMD, cache-friendly)

### 4. Reduce Memory Footprint

Once ADX is initialized, the `dx_values` vector can be dropped (only needed during warmup).

---

## Testing Recommendations

### 1. Compare Against Reference Implementation

Test against TA-Lib or TradingView ADX values.

### 2. Edge Case Tests

- All zeros
- Flat market (same OHLC values)
- Extreme gaps
- Single large movement
- Period boundary (exactly n and 2n-1 bars)

### 3. Numerical Stability

Run with 10,000+ bars and verify no drift or overflow.

### 4. Property-Based Testing

- ADX should always be in [0, 100]
- +DI and -DI should always be in [0, 100]
- +DI + -DI should always be <= 200

---

## References

- Wilder, J. Welles. "New Concepts in Technical Trading Systems" (1978)
- [StockCharts ChartSchool - ADX](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-indicators/average-directional-index-adx)
- [Macroption - ATR Calculation](https://www.macroption.com/atr-calculation/)
- [Tulip Indicators - Wilder's Smoothing](https://tulipindicators.org/wilders)
- [TradingView - ADX](https://www.tradingview.com/support/solutions/43000589099-average-directional-index-adx/)
- [BingX - ADX in Crypto Trading](https://bingx.com/en/learn/article/how-to-use-adx-indicator-in-crypto-trading)
- [The Robust Trader - ADX Settings](https://therobusttrader.com/adx-indicator-settings-best/)
- [Altrady - ADX Guide](https://www.altrady.com/crypto-trading/technical-analysis/average-directional-index-adx)
