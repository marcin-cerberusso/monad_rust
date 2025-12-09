// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trade history tracking and profit logging.

use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::{info, warn};

const TRADES_FILE: &str = "trades.json";

/// A record of a single trade (buy or sell).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub token: Address,
    pub token_name: String,
    pub token_symbol: String,
    pub trade_type: TradeType,
    pub amount_tokens: String, // U256 as string for serialization
    pub amount_mon: f64,
    pub timestamp: u64,
    pub tx_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeType {
    Buy,
    Sell,
}

/// Trade history tracker with persistence.
#[derive(Debug)]
pub struct TradeHistory {
    trades: Vec<TradeRecord>,
}

impl TradeHistory {
    /// Load trade history from file or create new.
    pub fn load() -> Self {
        let trades = match fs::read_to_string(TRADES_FILE) {
            Ok(contents) => {
                serde_json::from_str(&contents).unwrap_or_else(|e| {
                    warn!("Failed to parse trades.json: {}", e);
                    Vec::new()
                })
            }
            Err(_) => {
                info!("No trades history file found, starting fresh");
                Vec::new()
            }
        };
        
        info!("📊 Loaded {} historical trades", trades.len());
        Self { trades }
    }

    /// Save trade history to file.
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.trades)
            .map_err(|e| format!("Failed to serialize trades: {}", e))?;
        fs::write(TRADES_FILE, json)
            .map_err(|e| format!("Failed to write trades file: {}", e))?;
        Ok(())
    }

    /// Record a new trade.
    pub fn record(&mut self, trade: TradeRecord) {
        info!(
            "📝 Recording {}: {} {} ({}) - {:.4} MON",
            match trade.trade_type {
                TradeType::Buy => "BUY",
                TradeType::Sell => "SELL",
            },
            trade.amount_tokens,
            trade.token_symbol,
            trade.token_name,
            trade.amount_mon
        );
        
        self.trades.push(trade);
        
        if let Err(e) = self.save() {
            warn!("Failed to save trades: {}", e);
        }
    }

    /// Get profit/loss summary.
    pub fn get_summary(&self) -> TradeSummary {
        let mut total_bought = 0.0;
        let mut total_sold = 0.0;
        let mut buy_count = 0;
        let mut sell_count = 0;
        
        for trade in &self.trades {
            match trade.trade_type {
                TradeType::Buy => {
                    total_bought += trade.amount_mon;
                    buy_count += 1;
                }
                TradeType::Sell => {
                    total_sold += trade.amount_mon;
                    sell_count += 1;
                }
            }
        }
        
        TradeSummary {
            total_bought,
            total_sold,
            net_pnl: total_sold - total_bought,
            buy_count,
            sell_count,
        }
    }

    /// Log summary on startup.
    pub fn log_summary(&self) {
        let summary = self.get_summary();
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📊 Trade History Summary:");
        info!("   Buys: {} trades, {:.4} MON total", summary.buy_count, summary.total_bought);
        info!("   Sells: {} trades, {:.4} MON total", summary.sell_count, summary.total_sold);
        info!("   Net P/L: {:.4} MON", summary.net_pnl);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }
}

#[derive(Debug)]
pub struct TradeSummary {
    pub total_bought: f64,
    pub total_sold: f64,
    pub net_pnl: f64,
    pub buy_count: usize,
    pub sell_count: usize,
}
