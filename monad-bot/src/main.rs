// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Monad Sniper Bot - Fast token sniping for nad.fun

mod config;
mod executor;
mod listeners;
mod position;
mod rpc;
mod strategies;
mod validators;

use config::Config;
use executor::{SellExecutor, SwapExecutor};
use listeners::{spawn_listener, NewTokenEvent};
use position::{spawn_monitor, Position, PositionTracker, SellDecision, TrailingStopLossConfig};
use rpc::create_provider;
use strategies::SniperStrategy;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
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

    info!("🚀 Monad Sniper Bot starting...");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load config: {}", e);
        e
    })?;

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

    // Create swap executor (for buying)
    let buy_executor = SwapExecutor::new(provider.clone(), wallet.clone(), &config).await?;

    // Create sell executor
    let sell_executor = Arc::new(SellExecutor::new(provider.clone(), wallet, &config).await?);

    // Create strategy
    let strategy = SniperStrategy::from_config(&config);

    // Load existing positions into Arc<Mutex<>>
    let positions = Arc::new(Mutex::new(PositionTracker::load()));
    {
        let pos_guard = positions.lock().await;
        info!("📊 Loaded {} existing positions", pos_guard.len());
    }

    // Create channels
    let (new_token_tx, mut new_token_rx) = mpsc::channel::<NewTokenEvent>(100);
    let (sell_signal_tx, mut sell_signal_rx) = mpsc::channel::<(alloy::primitives::Address, SellDecision)>(100);

    // Start blockchain event listener
    info!("🔌 Connecting to Monad WebSocket for events...");
    let _listener_handle = spawn_listener(config.ws_url.clone(), new_token_tx);

    // Start position monitor (trailing stop-loss)
    let tsl_config = TrailingStopLossConfig::from_config(&config);
    let _monitor_handle = spawn_monitor(
        provider.clone(),
        config.router_address,
        config.wmon_address,
        tsl_config,
        Arc::clone(&positions),
        sell_signal_tx,
    );

    info!("✅ Sniper Bot ready! Waiting for new tokens...");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Spawn sell signal handler
    let positions_for_sell = Arc::clone(&positions);
    let sell_executor_clone = Arc::clone(&sell_executor);
    tokio::spawn(async move {
        while let Some((token, decision)) = sell_signal_rx.recv().await {
            info!("🔔 Processing sell signal for {:?}", token);
            
            let pos_guard = positions_for_sell.lock().await;
            if let Some(position) = pos_guard.get(&token) {
                let amount = position.amount;
                drop(pos_guard); // Release lock before async operation
                
                match sell_executor_clone.sell(token, amount, &decision).await {
                    Ok(tx_hash) => {
                        info!("✅ Sell executed: {:?}", tx_hash);
                        
                        // Remove or update position based on decision
                        let mut pos_guard = positions_for_sell.lock().await;
                        match &decision {
                            SellDecision::SecureProfit { portion, .. } => {
                                // Partial sell - update amount
                                if let Some(pos) = pos_guard.get_mut(&token) {
                                    let sold = pos.amount * alloy::primitives::U256::from((*portion * 100.0) as u64) / alloy::primitives::U256::from(100);
                                    pos.amount -= sold;
                                    info!("📊 Updated position: {} tokens remaining", pos.amount);
                                }
                            }
                            _ => {
                                // Full sell - remove position
                                pos_guard.remove(&token);
                                info!("📊 Position closed");
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Sell failed: {}", e);
                    }
                }
            }
        }
    });

    // Main event loop - handle new token events
    while let Some(token_event) = new_token_rx.recv().await {
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "🆕 New token: {} ({}) at {:?}",
            token_event.name, token_event.symbol, token_event.token_address
        );

        // Check if we should buy
        match strategy.should_buy(&token_event).await {
            Some(decision) => {
                // Execute buy
                match buy_executor.buy(&decision).await {
                    Ok(tx_hash) => {
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
                    }
                }
            }
            None => {
                warn!("⏭️ Skipping token: did not pass checks");
            }
        }
    }

    Ok(())
}
