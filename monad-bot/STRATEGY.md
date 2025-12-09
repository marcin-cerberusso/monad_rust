# 🎯 Monad Sniper Bot - Strategy & Architecture

## Executive Summary

Bot automatyzujący handel memecoinami na **nad.fun (Monad)**. Oparty na strategii Pump.fun z zaawansowanymi filtrami bezpieczeństwa i whale tracking.

---

## 📊 Token Categories (Meta Framework)

| Category | Driver | Lifespan | Bot Action |
|----------|--------|----------|------------|
| **Culture Coins** | Community | Long | Monitor, don't snipe |
| **Viral Trends** | Analytics + Catalyst | Medium | ✅ SNIPE on early dip |
| **Utility** | KOLs + Flywheels | Long | Monitor for entry |
| **News** | Twitter + MSM | Short | ✅ FAST SNIPE |
| **Gambles** | Off-Meta | Very Short | High risk, small size |
| **Cabal** | Insider groups | Variable | AVOID (bundling) |

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
│  │ nad.fun WS  │────▶│ Age < 30min │────▶│ Buy 5 MON   │       │
│  │ (new tokens)│     │ Dev < 8%    │     │ Slippage 5% │       │
│  └─────────────┘     │ MC 15k-25k  │     └──────┬──────┘       │
│         │            │ No bundling │            │               │
│         │            └─────────────┘            ▼               │
│  ┌─────────────┐                        ┌─────────────┐        │
│  │ QuickNode   │                        │ Position    │        │
│  │ Streams     │─────────────────────▶  │ Manager     │        │
│  │ (whales)    │                        └──────┬──────┘        │
│  └─────────────┘                               │               │
│                                                ▼               │
│                                        ┌─────────────┐        │
│                                        │ Exit Rules  │        │
│                                        │ MC > 40k TP │        │
│                                        │ -30% SL     │        │
│                                        │ Trail 20%   │        │
│                                        └─────────────┘        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🛡️ Validation Filters

### Filter 1: Token Age

```
IF token_age > 30 minutes:
    REJECT "Momentum lost"
```

### Filter 2: Dev Holdings

```
IF dev_wallet_balance / total_supply > 8%:
    REJECT "Rug pull risk"
```

### Filter 3: Market Cap Zone

```
IF market_cap < $15,000:
    REJECT "Too early, wait for momentum"
    
IF market_cap > $25,000:
    REJECT "Entry too late, risk/reward poor"
```

### Filter 4: Bundling Detection

```
FOR each top_holder:
    funding_source = get_first_incoming_tx(holder)
    
IF 3+ holders share same funding_source:
    REJECT "Coordinated manipulation"
    
IF majority holders have nonce = 0:
    REJECT "Fresh wallets = likely scam"
```

---

## 💰 Entry Strategy

### Value Zone: $15k - $25k Market Cap

```
┌────────────────────────────────────────────┐
│                                            │
│   ▲ Price                                  │
│   │                                        │
│   │        ╱╲                              │
│   │       ╱  ╲     Migration (~$50k)       │
│   │      ╱    ╲─── SELL HERE ────────      │
│   │     ╱      ╲                           │
│   │    ╱        ╲                          │
│   │   ╱          ╲                         │
│   │──╱── BUY HERE ╲── $15k-25k zone        │
│   │ ╱              ╲                       │
│   │╱                ╲_____                 │
│   └────────────────────────────▶ Time      │
│                                            │
└────────────────────────────────────────────┘
```

**Rules:**

1. Wait for first pullback (40-60% from local high)
2. RSI < 30 on 5-second timeframe = oversold
3. Volume increasing = confirmation

---

## 🎯 Exit Strategy

### Take Profit

| Market Cap | Action |
|------------|--------|
| $40,000 | Sell 50% |
| $50,000 | Sell remaining 50% (migration level) |

### Stop Loss

| Condition | Action |
|-----------|--------|
| -30% from entry | Hard stop, sell 100% |
| -20% from highest | Trailing stop (if profit > 50%) |
| No volume 5 min | Exit immediately |
| Max hold 48h | Force exit |

---

## 🐋 Whale Tracking (QuickNode Streams)

### Data Flow

```
QuickNode Streams
       │
       ▼ Webhook
┌─────────────────┐
│ ERC20 Transfers │
│    > 10k MON    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Whale Alert     │
│ Copy Trade?     │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
  BUY      IGNORE
(smart $)  (unknown)
```

### Smart Money Wallets

Track wallets with:

- Win rate > 60%
- Average ROI > 100%
- Account age > 30 days

---

## ⚡ MEV Protection

| Technique | Implementation |
|-----------|----------------|
| Aggressive Gas | 1.5x base fee |
| Private Mempool | QuickNode addon |
| Tight Slippage | 5% max |
| Fast Execution | < 100ms latency |

---

## 📁 Code Architecture

```
monad-bot/src/
├── main.rs              # Entry point, orchestration
├── config.rs            # Environment variables
├── listeners/
│   └── nadfun.rs        # WebSocket event listener
├── validators/
│   ├── token_analysis.rs # Dev holdings, market cap
│   ├── bundling.rs      # Coordinated wallet detection
│   ├── honeypot.rs      # Sell simulation
│   └── liquidity.rs     # Liquidity check
├── strategies/
│   └── sniper.rs        # Buy decision logic
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
    ├── kuru.rs          # Kuru DEX feed
    └── octoswap.rs      # OctoSwap feed
```

---

## 🔧 Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AUTO_SNIPE_AMOUNT_MON` | 5 | Amount per trade |
| `MAX_AGE_MINUTES` | 30 | Max token age |
| `MAX_DEV_HOLDING_PCT` | 8 | Max dev ownership |
| `MIN_MARKET_CAP_USD` | 15000 | Entry zone start |
| `MAX_MARKET_CAP_USD` | 25000 | Entry zone end |
| `TRAILING_DROP_PCT` | 20 | Trailing stop % |
| `HARD_STOP_LOSS_PCT` | -30 | Hard stop % |
| `SECURE_PROFIT_PCT` | 100 | Take profit trigger |

---

## 🚨 Risk Management

### Position Sizing

```
Max position = 1% of portfolio
Max concurrent = 5 positions
Daily loss limit = 10% of portfolio
```

### Red Flags (Auto-Reject)

- [ ] Dev holdings > 8%
- [ ] Token age > 30 min
- [ ] Same funding source for 3+ holders
- [ ] All holders have nonce = 0
- [ ] Name contains: test, scam, rug, honeypot

---

## 📈 Expected Performance

| Metric | Target |
|--------|--------|
| Win Rate | 40-50% |
| Avg Win | 2-4x |
| Avg Loss | -20-30% |
| Risk/Reward | 1:3 |
| Daily Trades | 5-15 |
