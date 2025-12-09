# Monad Trading Bot Strategies

Given the high-performance nature of Monad (10,000 TPS, 1s block time) and the low-latency access provided by the Execution Events SDK, the following strategies are most viable:

## 1. CEX-DEX Arbitrage (Latency Sensitive)
**Concept:** Exploit price differences between Monad DEXs (e.g., Uniswap v3 forks) and centralized exchanges (Binance, Coinbase).
**Why Monad?** The Execution Events SDK allows you to see state changes (reserves updates) *before* they might be fully propagated via public RPCs, giving you a speed advantage.
**Implementation:**
- **Listener:** `monad-bot` listens for `Log` events from known DEX pair addresses.
- **Decoder:** Decode `Sync` or `Swap` events to calculate new reserves/price.
- **Comparator:** Compare with real-time CEX websocket feeds.
- **Executor:** If spread > fees, execute swap on Monad and hedge on CEX.

## 2. Atomic Arbitrage (DEX-DEX)
**Concept:** Find cycles of trades across multiple DEXs on Monad that result in a profit (e.g., USDC -> MON -> WETH -> USDC).
**Why Monad?** High throughput means many opportunities, but also high competition. Low latency event reading is crucial to be the first to detect the imbalance.
**Implementation:**
- **Graph:** Maintain a local graph of all pools and prices.
- **Update:** Update graph edges immediately upon receiving `Swap` events.
- **Search:** Run Bellman-Ford or SPFA to find negative cycles (profit loops).
- **Execution:** Send a bundle/transaction to a smart contract that executes the swap chain atomically.

## 3. Liquidation Bot
**Concept:** Monitor lending protocols for under-collateralized positions and liquidate them for a bonus.
**Why Monad?** fast block times mean prices change quickly. Being the first to liquidate is a "winner takes all" game.
**Implementation:**
- **Tracking:** Maintain a local database of all open positions on lending protocols.
- **Price Feeds:** Listen to Oracle update events.
- **Trigger:** When an Oracle update pushes a user's health factor below 1, immediately submit a liquidation transaction.

## 4. Sniper Bot (New Pool Detection)
**Concept:** Buy into new token launches in the same block they are created.
**Why Monad?** Execution events allow you to see the `PairCreated` event instantly.
**Implementation:**
- **Listener:** Filter for `PairCreated` events from Factory contracts.
- **Action:** Simulate the token (check for honeypots) and buy immediately if safe.

## Recommended Architecture
1.  **Rust Event Reader (This Project):** Dedicated process to read shared memory ring buffer and decode events.
2.  **Strategy Engine:** Receives normalized events from the Reader, runs logic, and decides to trade.
3.  **Execution Client:** Sends signed transactions to the RPC / Sequencer.

## Next Steps
1.  **Decoder:** Implement specific event decoders for Uniswap V2/V3 `Sync` and `Swap` events.
2.  **RPC Connection:** Add an RPC client (e.g., `ethers-rs` or `alloy`) to send transactions.
3.  **Smart Contract:** Deploy an arbitrage executor contract.
