// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sell signal handler - processes trailing stop-loss and other sell signals.
//! Uses SDK for bonding curve tokens, DEX router for graduated tokens.
//! Features: rate limiting (30s cooldown), retry with higher slippage.

use crate::executor::{SdkExecutor, SellExecutor};
use crate::position::{PositionTracker, SellDecision};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

/// Cooldown between sell attempts for the same token (prevents spam).
const SELL_COOLDOWN_SECS: u64 = 30;

/// Spawn a background task to handle sell signals from the position monitor.
/// Uses SDK for bonding curve tokens, falls back to DEX router for graduated tokens.
/// Includes rate limiting (30s cooldown per token) and retry with higher slippage.
pub fn spawn_sell_handler<P: Provider + Clone + Send + Sync + 'static>(
    sdk_executor: Arc<SdkExecutor>,
    dex_sell_executor: Arc<SellExecutor<P>>,
    positions: Arc<Mutex<PositionTracker>>,
    mut sell_signal_rx: mpsc::Receiver<(Address, SellDecision)>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🔔 Sell signal handler started (SDK + DEX fallback, 30s cooldown)");
        
        // Track last sell attempt per token for rate limiting
        let mut last_sell_attempt: HashMap<Address, Instant> = HashMap::new();
        
        while let Some((token, decision)) = sell_signal_rx.recv().await {
            // Rate limiting: check if we've tried selling this token recently
            if let Some(last_attempt) = last_sell_attempt.get(&token) {
                let elapsed = last_attempt.elapsed();
                if elapsed < Duration::from_secs(SELL_COOLDOWN_SECS) {
                    let remaining = SELL_COOLDOWN_SECS - elapsed.as_secs();
                    info!(
                        "⏳ Skipping sell for {:?} - cooldown ({} sec remaining)",
                        token, remaining
                    );
                    continue;
                }
            }
            
            // Update last attempt time
            last_sell_attempt.insert(token, Instant::now());
            
            info!("🔔 Processing sell signal for {:?}", token);
            
            let pos_guard = positions.lock().await;
            if let Some(position) = pos_guard.get(&token) {
                let amount = position.amount;
                let name = position.name.clone();
                let symbol = position.symbol.clone();
                drop(pos_guard); // Release lock before async operation
                
                info!(
                    "🔴 Executing SELL: {} ({}) - {:?}",
                    name, symbol, decision
                );
                
                // Calculate sell amount based on decision
                let sell_amount = match &decision {
                    SellDecision::SecureProfit { portion, .. } => {
                        // Partial sell
                        amount * U256::from((*portion * 100.0) as u64) / U256::from(100)
                    }
                    _ => amount, // Full sell
                };
                
                // Try SDK first (for bonding curve tokens)
                let sdk_result = sdk_executor.sell_token(token, sell_amount).await;
                
                match sdk_result {
                    Ok(tx_hash) => {
                        info!("✅ SDK Sell executed: {}", tx_hash);
                        update_position_after_sell(&positions, token, &decision, amount).await;
                    }
                    Err(sdk_error) => {
                        warn!("⚠️ SDK sell failed: {}", sdk_error);
                        
                        // Retry with higher slippage (25%) - will be implemented in SDK executor
                        info!("🔄 Retrying SDK sell with higher slippage...");
                        match sdk_executor.sell_token_with_slippage(token, sell_amount, 25.0).await {
                            Ok(tx_hash) => {
                                info!("✅ SDK Sell (retry 25% slippage) executed: {}", tx_hash);
                                update_position_after_sell(&positions, token, &decision, amount).await;
                            }
                            Err(retry_error) => {
                                warn!("⚠️ SDK retry failed: {}, trying DEX...", retry_error);
                                
                                // Fallback to DEX router for graduated tokens
                                match dex_sell_executor.sell(token, sell_amount, &decision).await {
                                    Ok(tx_hash) => {
                                        info!("✅ DEX Sell executed: {:?}", tx_hash);
                                        update_position_after_sell(&positions, token, &decision, amount).await;
                                    }
                                    Err(dex_error) => {
                                        error!("❌ All sell attempts failed!");
                                        error!("   SDK (15%): {}", sdk_error);
                                        error!("   SDK (25%): {}", retry_error);
                                        error!("   DEX: {}", dex_error);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        info!("🔔 Sell signal handler stopped");
    })
}

async fn update_position_after_sell(
    positions: &Arc<Mutex<PositionTracker>>,
    token: Address,
    decision: &SellDecision,
    original_amount: U256,
) {
    let mut pos_guard = positions.lock().await;
    match decision {
        SellDecision::SecureProfit { portion, .. } => {
            // Partial sell - update amount
            if let Some(pos) = pos_guard.get_mut(&token) {
                let sold = original_amount * U256::from((*portion * 100.0) as u64) / U256::from(100);
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
