// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sniper strategy - decides whether to buy new tokens.

use crate::config::Config;
use crate::listeners::NewTokenEvent;
use crate::validators::{check_liquidity, liquidity::mon_to_wei};
use alloy::primitives::{Address, U256};
use tracing::{debug, info, warn};

/// Decision to buy a token.
#[derive(Debug, Clone)]
pub struct BuyDecision {
    pub token: Address,
    pub amount_wei: U256,
    pub name: String,
    pub symbol: String,
    pub reason: String,
}

/// Sniper strategy configuration and logic.
pub struct SniperStrategy {
    pub enabled: bool,
    pub min_liquidity_wei: u128,
    pub snipe_amount_wei: U256,
    pub whale_min_wei: U256,
    pub whale_max_wei: U256,
    pub ai_filter_enabled: bool,
    pub ai_min_score: u32,
    pub blacklist: Vec<String>,
}

impl SniperStrategy {
    /// Create strategy from config.
    pub fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.auto_snipe_enabled,
            min_liquidity_wei: mon_to_wei(10.0), // 10 MON minimum
            snipe_amount_wei: config.mon_to_wei(config.snipe_amount_mon),
            whale_min_wei: config.mon_to_wei(config.whale_min_amount),
            whale_max_wei: config.mon_to_wei(config.whale_max_amount),
            ai_filter_enabled: config.ai_filter_enabled,
            ai_min_score: config.ai_min_score,
            blacklist: config.blacklist.clone(),
        }
    }

    /// Evaluate whether to buy a new token.
    ///
    /// Returns `Some(BuyDecision)` if we should buy, `None` otherwise.
    pub async fn should_buy(&self, token: &NewTokenEvent) -> Option<BuyDecision> {
        if !self.enabled {
            debug!("Sniper disabled, skipping");
            return None;
        }

        // Check blacklist
        let name_lower = token.name.to_lowercase();
        let symbol_lower = token.symbol.to_lowercase();

        for word in &self.blacklist {
            if name_lower.contains(word) || symbol_lower.contains(word) {
                warn!(
                    "Token {} ({}) blacklisted: contains '{}'",
                    token.name, token.symbol, word
                );
                return None;
            }
        }

        // Check minimum name length
        if token.name.len() < 2 || token.symbol.len() < 1 {
            warn!("Token name/symbol too short: {} ({})", token.name, token.symbol);
            return None;
        }

        // Check liquidity
        if !check_liquidity(token.initial_liquidity, Some(self.min_liquidity_wei)) {
            warn!(
                "Token {} ({}) insufficient liquidity: {:?}",
                token.name, token.symbol, token.initial_liquidity
            );
            return None;
        }

        // Calculate buy amount
        // For now, use fixed snipe amount. Later can scale based on AI score.
        let amount = self.snipe_amount_wei;

        info!(
            "✅ BUY DECISION: {} ({}) - amount: {} wei",
            token.name, token.symbol, amount
        );

        Some(BuyDecision {
            token: token.token_address,
            amount_wei: amount,
            name: token.name.clone(),
            symbol: token.symbol.clone(),
            reason: "Passed all checks".to_string(),
        })
    }
}
