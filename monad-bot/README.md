# Monad Sniper Bot

High-performance token sniper for nad.fun on Monad blockchain.

## Features

- 🚀 **Real-time event monitoring** via QuickNode WebSocket
- 💰 **Automatic buying** with configurable amount
- 📉 **Trailing stop-loss** with configurable parameters
- 🛑 **Hard stop-loss** protection
- 💎 **Secure profit** partial sells
- ⏰ **Max hold time** enforcement

## Setup

### 1. Clone and build

```bash
cd monad-bot
cargo build --release
```

### 2. Configure

Copy `.env.example` to `.env` and fill in:

```bash
MONAD_RPC_URL=https://your-quicknode-endpoint
MONAD_WS_URL=wss://your-quicknode-endpoint
PRIVATE_KEY=your-private-key-without-0x
WALLET_ADDRESS=0x...
```

### 3. Run

```bash
./target/release/monad-bot
```

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `AUTO_SNIPE_AMOUNT_MON` | Amount per snipe | 5 |
| `TRAILING_DROP_PCT` | Trailing stop % | 20 |
| `TRAILING_MIN_PROFIT` | Min profit to trail | 50 |
| `HARD_STOP_LOSS_PCT` | Hard stop-loss % | -40 |
| `SECURE_PROFIT_PCT` | Profit to secure | 100 |
| `MAX_HOLD_HOURS` | Max hold time | 48 |

## Architecture

```
src/
├── main.rs          # Entry point
├── config.rs        # Configuration
├── listeners/       # Event sources
├── validators/      # Token validation
├── strategies/      # Buy decisions
├── executor/        # Transaction execution
└── position/        # Position management
```

## License

GPL-3.0
