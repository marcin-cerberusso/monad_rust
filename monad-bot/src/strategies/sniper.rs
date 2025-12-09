// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sniper strategy - decides whether to buy new tokens based on Pump.fun strategy.

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

/// Pump.fun style filter configuration.
#[derive(Debug, Clone)]
pub struct PumpFunFilters {
    /// Maximum token age in minutes (default: 30).
    pub max_age_minutes: u64,
    /// Maximum dev holding percentage (default: 8%).
    pub max_dev_holding_pct: f64,
    /// Maximum insider/sniper percentage (default: 25%).
    pub max_insider_pct: f64,
    /// Minimum market cap USD for entry zone (default: 15000).
    pub min_market_cap_usd: f64,
    /// Maximum market cap USD for entry zone (default: 25000).
    pub max_market_cap_usd: f64,
    /// MON price in USD (for market cap calculation).
    pub mon_price_usd: f64,
}

impl Default for PumpFunFilters {
    fn default() -> Self {
        Self {
            max_age_minutes: 30,
            max_dev_holding_pct: 8.0,
            max_insider_pct: 25.0,
            min_market_cap_usd: 15_000.0,
            max_market_cap_usd: 25_000.0,
            mon_price_usd: 0.50, // Estimate - should be fetched dynamically
        }
    }
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
    pub filters: PumpFunFilters,
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
            filters: PumpFunFilters::default(),
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

        // ========================================
        // FILTER 1: Blacklist check
        // ========================================
        let name_lower = token.name.to_lowercase();
        let symbol_lower = token.symbol.to_lowercase();

        for word in &self.blacklist {
            if name_lower.contains(word) || symbol_lower.contains(word) {
                warn!(
                    "❌ REJECTED [BLACKLIST]: {} ({}) contains '{}'",
                    token.name, token.symbol, word
                );
                return None;
            }
        }

        // ========================================
        // FILTER 2: Minimum name length
        // ========================================
        if token.name.len() < 2 || token.symbol.len() < 1 {
            warn!("❌ REJECTED [NAME]: {} ({}) - too short", token.name, token.symbol);
            return None;
        }

        // ========================================
        // FILTER 3: Liquidity check
        // ========================================
        if !check_liquidity(token.initial_liquidity, Some(self.min_liquidity_wei)) {
            warn!(
                "❌ REJECTED [LIQUIDITY]: {} ({}) - {:?} < {} wei",
                token.name, token.symbol, token.initial_liquidity, self.min_liquidity_wei
            );
            return None;
        }

        // ========================================
        // FILTER 4: Token Age (max 30 min)
        // ========================================
        let age_minutes = self.get_token_age_minutes(token);
        if age_minutes > self.filters.max_age_minutes {
            warn!(
                "❌ REJECTED [AGE]: {} ({}) - {} min > {} max",
                token.name, token.symbol, age_minutes, self.filters.max_age_minutes
            );
            return None;
        }

        // ========================================
        // FILTER 5: Market Cap Zone (15k-25k USD)
        // ========================================
        let market_cap_usd = self.estimate_market_cap(token);
        if market_cap_usd < self.filters.min_market_cap_usd {
            warn!(
                "❌ REJECTED [MCAP]: {} ({}) - ${:.0} < ${:.0} (too early)",
                token.name, token.symbol, market_cap_usd, self.filters.min_market_cap_usd
            );
            return None;
        }
        if market_cap_usd > self.filters.max_market_cap_usd {
            warn!(
                "❌ REJECTED [MCAP]: {} ({}) - ${:.0} > ${:.0} (too late)",
                token.name, token.symbol, market_cap_usd, self.filters.max_market_cap_usd
            );
            return None;
        }

        // ========================================
        // ALL FILTERS PASSED - BUY!
        // ========================================
        let amount = self.snipe_amount_wei;

        info!(
            "✅ BUY SIGNAL: {} ({}) | Age: {}min | MCap: ${:.0} | Amount: {} wei",
            token.name, token.symbol, age_minutes, market_cap_usd, amount
        );

        Some(BuyDecision {
            token: token.token_address,
            amount_wei: amount,
            name: token.name.clone(),
            symbol: token.symbol.clone(),
            reason: format!(
                "Passed all filters: age={}min, mcap=${:.0}",
                age_minutes, market_cap_usd
            ),
        })
    }

    /// Calculate token age in minutes.
    fn get_token_age_minutes(&self, token: &NewTokenEvent) -> u64 {
        let now = chrono::Utc::now().timestamp() as u64;
        if let Some(ts) = token.timestamp {
            if ts > 0 && ts < now {
                return (now - ts) / 60;
            }
        }
        0 // Fresh token
    }

    /// Estimate market cap in USD.
    /// MCap ≈ Liquidity * 2 (bonding curve approximation)
    fn estimate_market_cap(&self, token: &NewTokenEvent) -> f64 {
        let liquidity_wei = token.initial_liquidity
            .map(|l| l.to::<u128>())
            .unwrap_or(0);
        let liquidity_mon = liquidity_wei as f64 / 1e18;
        let liquidity_usd = liquidity_mon * self.filters.mon_price_usd;
        
        // Market cap ≈ 2x liquidity for bonding curve tokens
        liquidity_usd * 2.0
    }
}
