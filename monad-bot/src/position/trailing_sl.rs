// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trailing stop-loss implementation.

use crate::config::Config;
use crate::position::{Position, PositionTracker};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

// Router interface for price queries
sol! {
    #[sol(rpc)]
    interface IRouter {
        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
    }
}

/// Trailing stop-loss configuration.
#[derive(Debug, Clone)]
pub struct TrailingStopLossConfig {
    /// Percentage drop from highest to trigger sell.
    pub drop_pct: f64,
    /// Minimum profit percentage before trailing activates.
    pub min_profit_pct: f64,
    /// Hard stop-loss percentage (always triggers).
    pub hard_stop_loss_pct: f64,
    /// Profit percentage to secure partial profits.
    pub secure_profit_pct: f64,
    /// Portion to sell when securing profits.
    pub secure_sell_portion: f64,
    /// Maximum hold time in hours.
    pub max_hold_hours: u64,
    /// Check interval in seconds.
    pub check_interval_sec: u64,
}

impl TrailingStopLossConfig {
    pub fn from_config(config: &Config) -> Self {
        Self {
            drop_pct: config.trailing_drop_pct,
            min_profit_pct: config.trailing_min_profit,
            hard_stop_loss_pct: config.hard_stop_loss_pct,
            secure_profit_pct: config.secure_profit_pct,
            secure_sell_portion: config.secure_sell_portion,
            max_hold_hours: config.max_hold_hours,
            check_interval_sec: config.check_interval_sec,
        }
    }
}

/// Decision from trailing stop-loss check.
#[derive(Debug, Clone)]
pub enum SellDecision {
    /// Don't sell yet.
    Hold,
    /// Sell due to trailing stop triggered.
    TrailingStop { current_pnl: f64 },
    /// Sell due to hard stop-loss.
    HardStopLoss { current_pnl: f64 },
    /// Sell partial to secure profits.
    SecureProfit { portion: f64, current_pnl: f64 },
    /// Sell due to max hold time exceeded.
    MaxHoldTime { hours_held: u64 },
}

/// Position monitor that runs trailing stop-loss checks.
pub struct PositionMonitor<P: Provider + Clone> {
    provider: P,
    router: Address,
    wmon: Address,
    config: TrailingStopLossConfig,
}

impl<P: Provider + Clone + 'static> PositionMonitor<P> {
    pub fn new(
        provider: P,
        router: Address,
        wmon: Address,
        config: TrailingStopLossConfig,
    ) -> Self {
        Self {
            provider,
            router,
            wmon,
            config,
        }
    }

    /// Check a single position for sell conditions.
    pub async fn check_position(&self, position: &mut Position) -> SellDecision {
        // Get current price
        let current_price = match self.get_token_price_mon(position.token, position.amount).await {
            Ok(price) => price,
            Err(e) => {
                warn!("Failed to get price for {:?}: {}", position.token, e);
                return SellDecision::Hold;
            }
        };

        // Update highest price
        if current_price > position.highest_price {
            position.highest_price = current_price;
            debug!(
                "New high for {} ({}): {} MON",
                position.name, position.symbol, current_price
            );
        }

        // Calculate P&L
        let pnl_pct = if position.buy_price_mon > 0.0 {
            ((current_price - position.buy_price_mon) / position.buy_price_mon) * 100.0
        } else {
            0.0
        };

        debug!(
            "{} ({}) - Price: {} MON, P&L: {:.2}%, High: {} MON",
            position.name, position.symbol, current_price, pnl_pct, position.highest_price
        );

        // Check max hold time
        let now = chrono::Utc::now().timestamp() as u64;
        let hours_held = (now - position.buy_time) / 3600;
        if hours_held >= self.config.max_hold_hours {
            info!(
                "⏰ Max hold time exceeded for {} ({}) - {} hours",
                position.name, position.symbol, hours_held
            );
            return SellDecision::MaxHoldTime { hours_held };
        }

        // Check hard stop-loss (always active)
        if pnl_pct <= self.config.hard_stop_loss_pct {
            info!(
                "🛑 Hard stop-loss triggered for {} ({}) at {:.2}%",
                position.name, position.symbol, pnl_pct
            );
            return SellDecision::HardStopLoss { current_pnl: pnl_pct };
        }

        // Check secure profit (partial sell)
        if pnl_pct >= self.config.secure_profit_pct {
            info!(
                "💰 Secure profit triggered for {} ({}) at {:.2}%",
                position.name, position.symbol, pnl_pct
            );
            return SellDecision::SecureProfit {
                portion: self.config.secure_sell_portion,
                current_pnl: pnl_pct,
            };
        }

        // Check trailing stop (only if in profit above minimum)
        if pnl_pct >= self.config.min_profit_pct && position.highest_price > 0.0 {
            let drop_from_high = ((position.highest_price - current_price) / position.highest_price) * 100.0;
            
            if drop_from_high >= self.config.drop_pct {
                info!(
                    "📉 Trailing stop triggered for {} ({}) - dropped {:.2}% from high",
                    position.name, position.symbol, drop_from_high
                );
                return SellDecision::TrailingStop { current_pnl: pnl_pct };
            }
        }

        SellDecision::Hold
    }

    /// Get token price in MON.
    async fn get_token_price_mon(&self, token: Address, amount: U256) -> Result<f64, String> {
        let router = IRouter::new(self.router, &self.provider);
        let path = vec![token, self.wmon];

        let amounts = router
            .getAmountsOut(amount, path)
            .call()
            .await
            .map_err(|e| format!("getAmountsOut failed: {}", e))?;

        // Convert wei to MON
        let mon_wei = amounts[1];
        let mon = mon_wei.to::<u128>() as f64 / 1e18;
        
        Ok(mon)
    }
}

/// Spawn position monitor background task.
pub fn spawn_monitor<P: Provider + Clone + Send + Sync + 'static>(
    provider: P,
    router: Address,
    wmon: Address,
    config: TrailingStopLossConfig,
    positions: Arc<Mutex<PositionTracker>>,
    sell_tx: tokio::sync::mpsc::Sender<(Address, SellDecision)>,
) -> tokio::task::JoinHandle<()> {
    let interval_sec = config.check_interval_sec;
    let monitor = PositionMonitor::new(provider, router, wmon, config);
    
    tokio::spawn(async move {
        info!("📊 Position monitor started (checking every {}s)", interval_sec);
        
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_sec)).await;
            
            let mut positions_guard = positions.lock().await;
            let tokens: Vec<Address> = positions_guard.all().iter().map(|p| p.token).collect();
            
            for token in tokens {
                if let Some(position) = positions_guard.get_mut(&token) {
                    let decision = monitor.check_position(position).await;
                    
                    match &decision {
                        SellDecision::Hold => {}
                        _ => {
                            info!(
                                "🔔 Sell signal for {} ({}): {:?}",
                                position.name, position.symbol, decision
                            );
                            let _ = sell_tx.send((token, decision.clone())).await;
                        }
                    }
                }
            }
            
            // Save updated positions (highest_price may have changed)
            let _ = positions_guard.save();
        }
    })
}
