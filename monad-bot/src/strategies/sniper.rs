// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sniper strategy - optimized for nad.fun on Monad blockchain.
//! 
//! Key differences from Pump.fun (Solana):
//! - Migration MCap: ~$1.3M (vs $50k on Solana)
//! - Network: 10,000 TPS, 1s finality
//! - DEX: Capricorn CLMM

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

/// Monad/nad.fun specific filter configuration.
/// Optimized for Monad's bonding curve mechanics.
#[derive(Debug, Clone)]
pub struct MonadFilters {
    /// Maximum token age in minutes (default: 60 for Monad).
    pub max_age_minutes: u64,
    /// Maximum dev holding percentage (default: 10%).
    pub max_dev_holding_pct: f64,
    /// Maximum insider/sniper percentage (default: 30%).
    pub max_insider_pct: f64,
    /// Minimum market cap USD for entry zone (default: $50,000).
    pub min_market_cap_usd: f64,
    /// Maximum market cap USD for entry zone (default: $200,000).
    pub max_market_cap_usd: f64,
    /// Take profit target market cap (default: $500,000).
    pub take_profit_mcap_usd: f64,
    /// Migration market cap (~$1.3M on nad.fun).
    pub migration_mcap_usd: f64,
    /// MON price in USD (fetched dynamically ideally).
    pub mon_price_usd: f64,
    /// Fixed profit multiplier (2x-3x target).
    pub profit_target_multiplier: f64,
}

impl Default for MonadFilters {
    fn default() -> Self {
        Self {
            // Monad-specific parameters
            max_age_minutes: 60,           // Slower market than Solana
            max_dev_holding_pct: 10.0,     // Slightly more lenient
            max_insider_pct: 30.0,         // Higher threshold for Monad
            min_market_cap_usd: 50_000.0,  // Entry zone start
            max_market_cap_usd: 200_000.0, // Entry zone end
            take_profit_mcap_usd: 500_000.0, // TP target
            migration_mcap_usd: 1_300_000.0, // 80% sold = migration
            mon_price_usd: 0.50,           // ~$0.50 per MON estimate
            profit_target_multiplier: 2.5, // 2.5x target
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
    pub filters: MonadFilters,
}

impl SniperStrategy {
    /// Create strategy from config.
    pub fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.auto_snipe_enabled,
            min_liquidity_wei: mon_to_wei(100.0), // 100 MON minimum for Monad
            snipe_amount_wei: config.mon_to_wei(config.snipe_amount_mon),
            whale_min_wei: config.mon_to_wei(config.whale_min_amount),
            whale_max_wei: config.mon_to_wei(config.whale_max_amount),
            ai_filter_enabled: config.ai_filter_enabled,
            ai_min_score: config.ai_min_score,
            blacklist: config.blacklist.clone(),
            filters: MonadFilters::default(),
        }
    }

    /// Evaluate whether to buy a new token on nad.fun.
    ///
    /// Returns `Some(BuyDecision)` if we should buy, `None` otherwise.
    pub async fn should_buy(&self, token: &NewTokenEvent, analysis: &crate::validators::TokenAnalysis) -> Option<BuyDecision> {
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
                    "❌ REJECT [BLACKLIST]: {} ({}) contains '{}'",
                    token.name, token.symbol, word
                );
                return None;
            }
        }

        // ========================================
        // FILTER 2: Minimum name length
        // ========================================
        if token.name.len() < 2 || token.symbol.len() < 1 {
            warn!("❌ REJECT [NAME]: {} ({}) - too short", token.name, token.symbol);
            return None;
        }

        // ========================================
        // FILTER 3: Liquidity check (100 MON min for Monad)
        // ========================================
        if !check_liquidity(token.initial_liquidity, Some(self.min_liquidity_wei)) {
            warn!(
                "❌ REJECT [LIQUIDITY]: {} ({}) - below 100 MON minimum",
                token.name, token.symbol
            );
            return None;
        }

        // ========================================
        // FILTER 3.5: Safety Analysis (On-Chain)
        // ========================================
        if !analysis.is_safe {
            warn!(
                "❌ REJECT [SAFETY]: {} ({}) - Unsafe: {}",
                token.name, token.symbol, 
                analysis.rejection_reason.as_deref().unwrap_or("Unknown reason")
            );
            return None;
        }

        if analysis.dev_holding_pct > self.filters.max_dev_holding_pct {
            warn!(
                "❌ REJECT [DEV]: {} ({}) - Dev holds {:.1}% > {}%",
                token.name, token.symbol, 
                analysis.dev_holding_pct, self.filters.max_dev_holding_pct
            );
            return None;
        }

        // ========================================
        // FILTER 4: Token Age (max 60 min for Monad)
        // ========================================
        let age_minutes = self.get_token_age_minutes(token);
        if age_minutes > self.filters.max_age_minutes {
            warn!(
                "❌ REJECT [AGE]: {} ({}) - {} min > {} max",
                token.name, token.symbol, age_minutes, self.filters.max_age_minutes
            );
            return None;
        }

        // ========================================
        // FILTER 5: Market Cap Entry Zone ($50k-$200k)
        // ========================================
        let market_cap_usd = self.estimate_market_cap(token);
        
        if market_cap_usd < self.filters.min_market_cap_usd {
            info!(
                "⏳ WAIT [MCAP]: {} ({}) - ${:.0}k < ${:.0}k (too early, watching...)",
                token.name, token.symbol, 
                market_cap_usd / 1000.0, 
                self.filters.min_market_cap_usd / 1000.0
            );
            return None;
        }
        
        if market_cap_usd > self.filters.max_market_cap_usd {
            warn!(
                "❌ REJECT [MCAP]: {} ({}) - ${:.0}k > ${:.0}k (past entry zone)",
                token.name, token.symbol, 
                market_cap_usd / 1000.0, 
                self.filters.max_market_cap_usd / 1000.0
            );
            return None;
        }

        // ========================================
        // FILTER 6: Calculate potential profit
        // ========================================
        let potential_profit = self.filters.take_profit_mcap_usd / market_cap_usd;
        if potential_profit < 2.0 {
            warn!(
                "❌ REJECT [R/R]: {} ({}) - only {:.1}x potential (need 2x+)",
                token.name, token.symbol, potential_profit
            );
            return None;
        }

        // ========================================
        // ALL FILTERS PASSED - BUY SIGNAL!
        // ========================================
        let amount = self.snipe_amount_wei;
        let distance_to_migration = self.filters.migration_mcap_usd / market_cap_usd;

        info!(
            "🟢 BUY SIGNAL: {} ({}) | MCap: ${:.0}k | Age: {}min | Potential: {:.1}x | To Migration: {:.1}x",
            token.name, token.symbol, 
            market_cap_usd / 1000.0,
            age_minutes, 
            potential_profit,
            distance_to_migration
        );

        Some(BuyDecision {
            token: token.token_address,
            amount_wei: amount,
            name: token.name.clone(),
            symbol: token.symbol.clone(),
            reason: format!(
                "Entry at ${:.0}k mcap, {:.1}x potential, {:.1}x to migration",
                market_cap_usd / 1000.0, potential_profit, distance_to_migration
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
    /// For nad.fun bonding curve: MCap ≈ Liquidity * 2
    /// Migration happens at ~$1.3M when 80% tokens sold
    fn estimate_market_cap(&self, token: &NewTokenEvent) -> f64 {
        let liquidity_wei = token.initial_liquidity
            .map(|l| l.to::<u128>())
            .unwrap_or(0);
        let liquidity_mon = liquidity_wei as f64 / 1e18;
        let liquidity_usd = liquidity_mon * self.filters.mon_price_usd;
        
        // Market cap ≈ 2x liquidity for bonding curve tokens
        liquidity_usd * 2.0
    }

    /// Check if token should be sold (for exit strategy).
    pub fn should_take_profit(&self, current_mcap_usd: f64, entry_mcap_usd: f64) -> bool {
        // 2.5x profit target
        current_mcap_usd >= entry_mcap_usd * self.filters.profit_target_multiplier
    }

    /// Check if approaching migration (80% sold).
    pub fn is_near_migration(&self, current_mcap_usd: f64) -> bool {
        current_mcap_usd >= self.filters.migration_mcap_usd * 0.8
    }
}
