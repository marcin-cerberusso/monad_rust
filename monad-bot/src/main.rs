// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Monad Sniper Bot - Fast token sniping for nad.fun

mod arbitrage;
mod config;
mod executor;
mod handlers;
mod listeners;
mod position;
mod rpc;
mod strategies;
mod streams;
mod trade_history;
mod validators;
mod telegram;

use config::Config;
use executor::{SdkExecutor, SellExecutor, SwapExecutor};
use handlers::spawn_sell_handler;
use listeners::{spawn_listener, NewTokenEvent, CopyTradeEvent};
use telegram::TelegramNotifier;
use position::{spawn_monitor, Position, PositionTracker, SellDecision, TrailingStopLossConfig};
use rpc::create_provider;
use strategies::SniperStrategy;
use validators::{TokenAnalyzer, FilterConfig};

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::signal;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Load configuration for potential test mode
    let config_for_test = Config::from_env().map_err(|e| {
        error!("Failed to load config for test mode: {}", e);
        e
    });

    // Check for test mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--test-analysis" {
        let config = config_for_test?; // Use the loaded config
        let token_addr: alloy::primitives::Address = args.get(2)
            .expect("Provide token address")
            .parse()
            .expect("Invalid address");
            
        info!("🧪 Testing analysis for {:?}", token_addr);
        
        let (provider, _) = create_provider(&rpc::RpcConfig {
            rpc_url: config.rpc_url.clone(),
            private_key: config.private_key.clone(),
            chain_id: config.chain_id,
        })?;
        let filter_config = crate::validators::FilterConfig::default();
        let analyzer = crate::validators::TokenAnalyzer::new(provider, filter_config, 0.50);
        
        let analysis = analyzer.analyze(token_addr, None, 0, 1000.0).await;
        info!("📊 Results: {:?}", analysis);
        if analysis.total_supply > alloy::primitives::U256::ZERO {
            info!("✅ RPC Connection & Contract Call Successful");
        } else {
            error!("❌ Failed to get data");
        }
        return Ok(());
    }

    info!("🚀 Monad Sniper Bot starting...");

    // Load configuration (main execution)
    let config = config_for_test?; // Use the already loaded config

    info!("📡 RPC: {}", config.rpc_url);
    info!("📡 WS:  {}", config.ws_url);
    info!("👛 Wallet: {:?}", config.wallet_address);
    info!("💰 Snipe amount: {} MON", config.snipe_amount_mon);
    info!("📉 Trailing SL: {}% drop, {}% min profit", config.trailing_drop_pct, config.trailing_min_profit);

    // Create provider and wallet
    let (provider, wallet) = create_provider(&rpc::RpcConfig {
        rpc_url: config.rpc_url.clone(),
        private_key: config.private_key.clone(),
        chain_id: config.chain_id,
    })?;

    info!("✅ Connected to Monad RPC");

    // Create swap executor (for buying new tokens via DEX)
    let buy_executor = SwapExecutor::new(provider.clone(), wallet.clone(), &config).await?;

    // Create SDK executor (for bonding curve trades - copy trading)
    let sdk_executor = Arc::new(
        SdkExecutor::new(
            config.rpc_url.clone(),
            config.private_key.clone(),
            5.0, // 5% slippage for copy trades
        ).await?
    );

    // Create sell executor
    let sell_executor = Arc::new(SellExecutor::new(provider.clone(), wallet, &config).await?);

    // Create strategy
    let strategy = SniperStrategy::from_config(&config);

    // Create token analyzer
    let analyzer = TokenAnalyzer::new(
        provider.clone(),
        FilterConfig::default(),
        0.50, // TODO: Fetch price dynamically or from config
    );

    // Load existing positions into Arc<Mutex<>>
    let positions = Arc::new(Mutex::new(PositionTracker::load()));
    {
        let pos_guard = positions.lock().await;
        info!("📊 Loaded {} existing positions", pos_guard.len());
    }

    // Create channels
    let (new_token_tx, mut new_token_rx) = mpsc::channel::<NewTokenEvent>(100);
    let (sell_signal_tx, sell_signal_rx) = mpsc::channel::<(alloy::primitives::Address, SellDecision)>(100);
    let (copy_trade_tx, mut copy_trade_rx) = mpsc::channel::<CopyTradeEvent>(100);

    // Start blockchain event listener
    info!("🔌 Connecting to Monad WebSocket for events...");
    let _listener_handle = spawn_listener(
        config.ws_url.clone(), 
        new_token_tx,
        copy_trade_tx,
        config.smart_wallets.clone(),
    );

    // Start position monitor (trailing stop-loss) with SDK pricing
    let tsl_config = TrailingStopLossConfig::from_config(&config);
    let _monitor_handle = spawn_monitor(
        provider.clone(),
        config.router_address,
        config.wmon_address,
        Arc::clone(&sdk_executor),
        tsl_config,
        Arc::clone(&positions),
        sell_signal_tx,
    );

    // Initialize Telegram notifier
    let telegram = Arc::new(TelegramNotifier::new(
        config.telegram_token.clone(),
        config.telegram_chat_id.clone(),
    ));

    telegram.send_message("🚀 Monad Sniper Bot launching...").await;

    // Start arbitrage scanner
    let (arb_tx, _) = mpsc::channel::<arbitrage::ArbitrageOpportunity>(100);
    
    if config.arbitrage_enabled {
        let pairs = vec![
            arbitrage::TokenPair {
                token_a: config.wmon_address,
                token_b: "0x0F0BDEbF0F83cD1EE3974779Bcb7315f9808c714".parse().unwrap(), // USDC
                name: "WMON/USDC".to_string(),
            },
            arbitrage::TokenPair {
                token_a: config.wmon_address,
                token_b: "0xf817257fed379853cDe0fa4F97AB987181B1E5Ea".parse().unwrap(), // USDT  
                name: "WMON/USDT".to_string(),
            },
        ];
        
        let scan_amount = config.mon_to_wei(config.arb_amount_mon);
        let _arb_handle = arbitrage::spawn_scanner(
            provider.clone(),
            pairs,
            scan_amount,
            config.arb_scan_interval_ms,
            arb_tx,
        );
        info!("🔍 Arbitrage scanner enabled ({}ms interval, {} MON)", 
              config.arb_scan_interval_ms, config.arb_amount_mon);
    }


    info!("✅ Sniper Bot ready! Waiting for new tokens...");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Start Mempool Monitor (Front-running)
    if !config.smart_wallets.is_empty() {
        let mempool = listeners::mempool::MempoolMonitor::new(config.clone(), Arc::clone(&sdk_executor));
        tokio::spawn(async move {
            mempool.start().await;
        });
        info!("🦈 Mempool Monitor started (Front-running enabled)");
    }

    // Spawn sell signal handler (SDK for bonding curve, DEX fallback)
    let _sell_handler = spawn_sell_handler(
        Arc::clone(&sdk_executor),
        Arc::clone(&sell_executor),
        Arc::clone(&positions),
        sell_signal_rx,
    );

    // Clone positions for shutdown handler
    let positions_for_shutdown = Arc::clone(&positions);

    // Main event loop with graceful shutdown
    loop {
        tokio::select! {
            // Handle shutdown signal
            _ = signal::ctrl_c() => {
                info!("🛑 Shutdown signal received, saving positions...");
                let pos_guard = positions_for_shutdown.lock().await;
                if let Err(e) = pos_guard.save() {
                    error!("❌ Failed to save positions: {}", e);
                } else {
                    info!("✅ Positions saved successfully ({} positions)", pos_guard.len());
                }
                telegram.send_message("🛑 Bot shutting down gracefully...").await;
                break;
            }
            
            // Handle new token events
            Some(token_event) = new_token_rx.recv() => {
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                let name = token_event.name.clone();
                let symbol = token_event.symbol.clone();

                info!(
                    "🆕 New token: {} ({}) at {:?}",
                    name, symbol, token_event.token_address
                );

                // Analyze token
                let liquidity_mon = token_event.initial_liquidity
                    .map(|l| l.to::<u128>() as f64 / 1e18)
                    .unwrap_or(0.0);

                let analysis = analyzer.analyze(
                    token_event.token_address,
                    token_event.creator,
                    token_event.timestamp.unwrap_or(0),
                    liquidity_mon
                ).await;

                info!("🛡️ Analysis: Safe={}, Dev={:.1}%", analysis.is_safe, analysis.dev_holding_pct);

                // Map to NewTokenEvent for Strategy
                let strategy_event = NewTokenEvent {
                    token_address: token_event.token_address,
                    name: name.clone(),
                    symbol: symbol.clone(),
                    creator: token_event.creator,
                    bonding_curve: None,
                    initial_liquidity: token_event.initial_liquidity,
                    timestamp: token_event.timestamp,
                    tx_hash: token_event.tx_hash,
                };

                // Send Telegram notification for new token
                telegram.send_message(&format!(
                    "🆕 *New Token Detected*\nName: {}\nSymbol: {}\nAddress: `{:?}`", 
                    name, symbol, token_event.token_address
                )).await;

                // Check if we should buy
                match strategy.should_buy(&strategy_event, &analysis).await {
                    Some(decision) => {
                        // Execute buy
                        match buy_executor.buy(&decision).await {
                            Ok(tx_hash) => {
                                let msg = format!("🟢 *BUY EXECUTED*\nToken: {}\nHash: `{:?}`", decision.symbol, tx_hash);
                                telegram.send_message(&msg).await;
                                
                                // Calculate buy price (amount in MON)
                                let buy_price = decision.amount_wei.to::<u128>() as f64 / 1e18;
                                
                                // Add to positions
                                let position = Position {
                                    token: decision.token,
                                    name: decision.name,
                                    symbol: decision.symbol,
                                    amount: decision.amount_wei, // This will be updated with actual token amount
                                    buy_price_mon: buy_price,
                                    buy_time: chrono::Utc::now().timestamp() as u64,
                                    highest_price: buy_price,
                                    tx_hash: format!("{:?}", tx_hash),
                                };
                                
                                let mut pos_guard = positions.lock().await;
                                pos_guard.add(position);
                            }
                            Err(e) => {
                                error!("❌ Buy failed: {}", e);
                                telegram.send_message(&format!("❌ *Buy Failed*\nError: {}", e)).await;
                            }
                        }
                    }
                    None => {
                        warn!("⏭️ Skipping token: did not pass checks");
                    }
                }
            }
            
            // Handle copy trade events from smart wallets
            Some(copy_event) = copy_trade_rx.recv() => {
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                
                if copy_event.is_buy {
                    info!(
                        "📋 COPY TRADE BUY: {:?} | Smart Wallet: {:?} | Amount: {}",
                        copy_event.token, copy_event.smart_wallet, copy_event.amount_in
                    );
                    
                    // Send Telegram notification
                    telegram.send_message(&format!(
                        "📋 *COPY TRADE*\nSmart wallet `{:?}` bought token\nToken: `{:?}`\nExecuting copy buy via SDK...", 
                        copy_event.smart_wallet, copy_event.token
                    )).await;
                    
                    // Use SDK executor for bonding curve trades (not DEX router!)
                    let buy_amount = config.mon_to_wei(config.snipe_amount_mon);
                    
                    match sdk_executor.buy_token(copy_event.token, buy_amount).await {
                        Ok(tx_hash) => {
                            let msg = format!("🟢 *COPY BUY EXECUTED*\nToken: `{:?}`\nHash: `{}`", copy_event.token, tx_hash);
                            telegram.send_message(&msg).await;
                            info!("✅ Copy trade executed via SDK: {}", tx_hash);
                            
                            // Get actual token balance received
                            let token_balance = match sdk_executor.get_token_balance(copy_event.token).await {
                                Ok(balance) => {
                                    info!("📊 Received {} tokens", balance);
                                    balance
                                }
                                Err(e) => {
                                    warn!("⚠️ Couldn't get token balance: {}, using estimate", e);
                                    buy_amount // Fallback to buy amount if balance check fails
                                }
                            };
                            
                            // Fetch real token name and symbol from chain
                            let (token_name, token_symbol) = match sdk_executor.get_token_info(copy_event.token).await {
                                Ok((name, symbol)) => {
                                    info!("📝 Token info: {} ({})", name, symbol);
                                    (name, symbol)
                                }
                                Err(_) => {
                                    (format!("CopyTrade-{:?}", copy_event.token), "COPY".to_string())
                                }
                            };
                            
                            // Add to positions with actual token info
                            let buy_price = config.snipe_amount_mon;
                            let position = Position {
                                token: copy_event.token,
                                name: token_name,
                                symbol: token_symbol,
                                amount: token_balance, // Actual tokens received!
                                buy_price_mon: buy_price,
                                buy_time: chrono::Utc::now().timestamp() as u64,
                                highest_price: buy_price,
                                tx_hash: tx_hash.clone(),
                            };
                            
                            let mut pos_guard = positions.lock().await;
                            pos_guard.add(position);
                        }
                        Err(e) => {
                            error!("❌ Copy trade buy failed: {}", e);
                            telegram.send_message(&format!("❌ *Copy Trade Failed*\nError: {}", e)).await;
                        }
                    }
                } else {
                    // Smart wallet selling - we should also consider selling
                    info!(
                        "📋 COPY TRADE SELL SIGNAL: {:?} | Smart Wallet: {:?}",
                        copy_event.token, copy_event.smart_wallet
                    );
                    
                    // Check if we have this position
                    let pos_guard = positions.lock().await;
                    if pos_guard.get(&copy_event.token).is_some() {
                        drop(pos_guard);
                        
                        telegram.send_message(&format!(
                            "⚠️ *Smart Wallet Selling*\nWallet `{:?}` is selling token `{:?}`\nConsider exiting position!", 
                            copy_event.smart_wallet, copy_event.token
                        )).await;
                    }
                }
            }
        }
    }

    Ok(())
}
