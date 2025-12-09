# 🎯 Monad Sniper Bot - Strategy & Architecture

## Executive Summary

Bot automatyzujący handel memecoinami na **nad.fun (Monad)**. Zoptymalizowany dla specyfiki Monad blockchain z wysoką przepustowością i szybką finalnością.

---

## ⚡ Monad vs Solana (Key Differences)

| Parametr | Pump.fun (Solana) | **nad.fun (Monad)** |
|----------|-------------------|---------------------|
| **TPS** | ~400 | **10,000** |
| **Block Time** | ~400ms | **400ms** |
| **Finality** | ~12s | **~800ms** |
| **Migration MCap** | ~$50k | **~$1.3M** (80% sold) |
| **Entry Zone** | $15k-$25k | **$50k-$200k** |
| **Take Profit** | $40k-$50k | **$500k-$1M** |
| **DEX** | Raydium | **Capricorn CLMM** |

---

## 🔄 Trading Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    MONAD SNIPER BOT FLOW                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ DATA SOURCE │     │ VALIDATION  │     │  EXECUTION  │       │
│  └──────┬──────┘     └──────┬──────┘     └──────┬──────┘       │
│         │                   │                   │               │
│         ▼                   ▼                   ▼               │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │ nad.fun WS  │────▶│ Age < 60min │────▶│ Buy 10 MON  │       │
│  │ Moralis API │     │ Dev < 10%   │     │ Slippage 5% │       │
│  │ QuickNode   │     │ MC $50k-200k│     └──────┬──────┘       │
│  └─────────────┘     │ No bundling │            │               │
│         │            └─────────────┘            ▼               │
│         │                                ┌─────────────┐        │
│  ┌─────────────┐                        │ Position    │        │
│  │ Whale       │───────────────────────▶│ Manager     │        │
│  │ Tracking    │                        └──────┬──────┘        │
│  └─────────────┘                               │               │
│                                                ▼               │
│                                        ┌─────────────┐        │
│                                        │ Exit Rules  │        │
│                                        │ 2.5x TP     │        │
│                                        │ -30% SL     │        │
│                                        │ $1.3M migr  │        │
│                                        └─────────────┘        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🛡️ Validation Filters (Monad-Optimized)

### Filter 1: Token Age

```
IF token_age > 60 minutes:
    REJECT "Momentum lost"
```

### Filter 2: Dev Holdings

```
IF dev_wallet_balance / total_supply > 10%:
    REJECT "Rug pull risk"
```

### Filter 3: Market Cap Entry Zone

```
IF market_cap < $50,000:
    WAIT "Too early, monitoring..."
    
IF market_cap > $200,000:
    REJECT "Past entry zone"
```

### Filter 4: Risk/Reward Check

```
potential_profit = take_profit_mcap / current_mcap
IF potential_profit < 2x:
    REJECT "Insufficient upside"
```

### Filter 5: Bundling Detection

```
FOR each top_holder:
    funding_source = get_first_incoming_tx(holder)
    
IF 3+ holders share same funding_source:
    REJECT "Coordinated manipulation"
```

---

## 💰 Entry Strategy (Value Zone)

### Entry Zone: $50k - $200k Market Cap

```
┌────────────────────────────────────────────────────────────┐
│                                                            │
│   ▲ Market Cap                                             │
│   │                                                        │
│   │  $1.3M ─────────────────── MIGRATION (80% sold)        │
│   │         ╱                                              │
│   │  $500k ╱────────────────── TAKE PROFIT (2.5x)          │
│   │       ╱                                                │
│   │ $200k╱─────────────────── MAX ENTRY                    │
│   │     ╱                                                  │
│   │    ╱                                                   │
│   │   ╱                                                    │
│   │$50k ────────────────────── MIN ENTRY                   │
│   │  ╱                                                     │
│   └────────────────────────────────────────▶ Time          │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

---

## 🎯 Exit Strategy

### Take Profit

| Market Cap | Action |
|------------|--------|
| 2.5x from entry | Sell 50% |
| $500k+ | Sell 75% |
| $1M+ | Sell remaining (approaching migration) |

### Stop Loss

| Condition | Action |
|-----------|--------|
| -30% from entry | Hard stop, sell 100% |
| -20% from highest | Trailing stop (if profit > 50%) |
| No volume 10 min | Exit immediately |
| Max hold 24h | Force exit |

---

## 🐋 Data Sources

### Primary: QuickNode Pro

- WebSocket for nad.fun events
- Streams for whale tracking
- Low latency RPC

### Secondary: Moralis API

```
Moralis Monad Support:
- Wallet balances & history
- Token transfers tracking
- NFT data
- Streams for real-time events
```

### Backup: Alchemy

- Monad mainnet RPC
- Transaction history queries

---

## 📁 Code Architecture

```
monad-bot/src/
├── main.rs              # Entry point
├── config.rs            # Environment variables
├── listeners/
│   └── nadfun.rs        # WebSocket event listener
├── validators/
│   ├── token_analysis.rs # Dev holdings, market cap
│   ├── bundling.rs      # Coordinated wallet detection
│   ├── honeypot.rs      # Sell simulation
│   └── liquidity.rs     # Liquidity check
├── strategies/
│   └── sniper.rs        # Monad-optimized buy logic
├── executor/
│   ├── swap.rs          # Buy transactions
│   ├── sell.rs          # Sell transactions
│   └── gas.rs           # Gas strategies
├── position/
│   ├── tracker.rs       # Position management
│   └── trailing_sl.rs   # Stop-loss logic
├── streams/
│   └── webhook.rs       # QuickNode Streams
└── arbitrage/
    ├── scanner.rs       # Price comparison
    └── *.rs             # DEX integrations
```

---

## 🔧 Configuration (Monad-Optimized)

| Variable | Default | Description |
|----------|---------|-------------|
| `AUTO_SNIPE_AMOUNT_MON` | 10 | Amount per trade |
| `MAX_AGE_MINUTES` | 60 | Max token age |
| `MAX_DEV_HOLDING_PCT` | 10 | Max dev ownership |
| `MIN_MARKET_CAP_USD` | 50000 | Entry zone start |
| `MAX_MARKET_CAP_USD` | 200000 | Entry zone end |
| `TAKE_PROFIT_MCAP` | 500000 | TP target |
| `MIGRATION_MCAP` | 1300000 | Migration threshold |
| `PROFIT_MULTIPLIER` | 2.5 | Target profit |
| `HARD_STOP_LOSS_PCT` | -30 | Hard stop % |

---

## 🚨 Risk Management

### Position Sizing

```
Max position = 2% of portfolio
Max concurrent = 3 positions
Daily loss limit = 15% of portfolio
```

### Red Flags (Auto-Reject)

- [ ] Dev holdings > 10%
- [ ] Token age > 60 min
- [ ] Market cap > $200k
- [ ] Same funding source for 3+ holders
- [ ] Name contains: test, scam, rug, honeypot

---

## 📈 Expected Performance (Monad)

| Metric | Target |
|--------|--------|
| Win Rate | 35-45% |
| Avg Win | 2-3x |
| Avg Loss | -25% |
| Risk/Reward | 1:4 |
| Daily Trades | 3-8 |
| Monthly ROI | 50-100% |

---

## 🔗 APIs & Integrations

| Service | Purpose | Status |
|---------|---------|--------|
| QuickNode Pro | RPC + WebSocket + Streams | ✅ Active |
| Moralis | Wallet data, transfers, events | 🔧 To integrate |
| Alchemy | Backup RPC | ✅ Configured |
| nad.fun | Token events | ✅ Monitoring |
| Capricorn | DEX swaps | ✅ Ready |
