// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Token analysis for filtering scams and low-quality projects.
//! NOTE: This module is prepared for future integration.

// #![allow(unused)]

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;
use tracing::{info, warn};

/// Token analysis result.
#[derive(Debug, Clone)]
pub struct TokenAnalysis {
    pub token: Address,
    pub dev_wallet: Option<Address>,
    pub dev_holding_pct: f64,
    pub top_holder_pct: f64,
    pub total_supply: U256,
    pub market_cap_usd: f64,
    pub age_minutes: u64,
    pub is_safe: bool,
    pub rejection_reason: Option<String>,
}

/// Filter configuration.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Maximum age in minutes (default: 30).
    pub max_age_minutes: u64,
    /// Maximum dev holding percentage (default: 8%).
    pub max_dev_holding_pct: f64,
    /// Maximum sniper/insider percentage (default: 25%).
    pub max_insider_pct: f64,
    /// Minimum market cap USD (default: 15000).
    pub min_market_cap_usd: f64,
    /// Maximum market cap USD (default: 25000).
    pub max_market_cap_usd: f64,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            max_age_minutes: 30,
            max_dev_holding_pct: 8.0,
            max_insider_pct: 25.0,
            min_market_cap_usd: 15_000.0,
            max_market_cap_usd: 25_000.0,
        }
    }
}

// ERC20 interface for balance queries
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function totalSupply() external view returns (uint256);
        function decimals() external view returns (uint8);
    }
}

/// Token analyzer.
pub struct TokenAnalyzer<P: Provider + Clone> {
    provider: P,
    config: FilterConfig,
    mon_price_usd: f64,
}

impl<P: Provider + Clone> TokenAnalyzer<P> {
    pub fn new(provider: P, config: FilterConfig, mon_price_usd: f64) -> Self {
        Self {
            provider,
            config,
            mon_price_usd,
        }
    }

    /// Analyze a token for safety.
    pub async fn analyze(
        &self,
        token: Address,
        dev_wallet: Option<Address>,
        creation_time: u64,
        liquidity_mon: f64,
    ) -> TokenAnalysis {
        let now = chrono::Utc::now().timestamp() as u64;
        let age_minutes = (now - creation_time) / 60;

        // Get token contract
        let contract = IERC20::new(token, &self.provider);

        // Get total supply
        let total_supply = match contract.totalSupply().call().await {
            Ok(supply) => supply,
            Err(e) => {
                warn!("Failed to get total supply: {}", e);
                return self.reject(token, "Failed to get total supply");
            }
        };

        // If liquidity not provided, use default estimate for new launch (~85 MON)
        let liquidity_used = if liquidity_mon > 0.0 {
            liquidity_mon
        } else {
            85.0
        };

        // Calculate market cap (liquidity * 2 is rough estimate)
        let market_cap_usd = liquidity_used * self.mon_price_usd * 2.0;

        // Check dev holdings if dev wallet provided
        let dev_holding_pct = if let Some(dev) = dev_wallet {
            match contract.balanceOf(dev).call().await {
                Ok(balance) => {
                    if total_supply > U256::ZERO {
                        let pct = (balance.to::<u128>() as f64 / total_supply.to::<u128>() as f64) * 100.0;
                        pct
                    } else {
                        0.0
                    }
                }
                Err(_) => 0.0,
            }
        } else {
            0.0
        };

        // Check age filter
        if age_minutes > self.config.max_age_minutes {
            return self.reject_with_analysis(
                token, dev_wallet, dev_holding_pct, 0.0, total_supply, market_cap_usd, age_minutes,
                format!("Token too old: {} min > {} max", age_minutes, self.config.max_age_minutes)
            );
        }

        // Check dev holdings
        if dev_holding_pct > self.config.max_dev_holding_pct {
            return self.reject_with_analysis(
                token, dev_wallet, dev_holding_pct, 0.0, total_supply, market_cap_usd, age_minutes,
                format!("Dev holdings too high: {:.1}% > {}%", dev_holding_pct, self.config.max_dev_holding_pct)
            );
        }

        // Check market cap zone
        if market_cap_usd < self.config.min_market_cap_usd {
            return self.reject_with_analysis(
                token, dev_wallet, dev_holding_pct, 0.0, total_supply, market_cap_usd, age_minutes,
                format!("Market cap too low: ${:.0} < ${:.0}", market_cap_usd, self.config.min_market_cap_usd)
            );
        }

        if market_cap_usd > self.config.max_market_cap_usd {
            return self.reject_with_analysis(
                token, dev_wallet, dev_holding_pct, 0.0, total_supply, market_cap_usd, age_minutes,
                format!("Market cap too high: ${:.0} > ${:.0}", market_cap_usd, self.config.max_market_cap_usd)
            );
        }

        info!(
            "✅ Token passed filters: age={}min, dev={:.1}%, mcap=${:.0}",
            age_minutes, dev_holding_pct, market_cap_usd
        );

        TokenAnalysis {
            token,
            dev_wallet,
            dev_holding_pct,
            top_holder_pct: 0.0, // TODO: implement top holder analysis
            total_supply,
            market_cap_usd,
            age_minutes,
            is_safe: true,
            rejection_reason: None,
        }
    }

    fn reject(&self, token: Address, reason: &str) -> TokenAnalysis {
        warn!("❌ Token rejected: {}", reason);
        TokenAnalysis {
            token,
            dev_wallet: None,
            dev_holding_pct: 0.0,
            top_holder_pct: 0.0,
            total_supply: U256::ZERO,
            market_cap_usd: 0.0,
            age_minutes: 0,
            is_safe: false,
            rejection_reason: Some(reason.to_string()),
        }
    }

    fn reject_with_analysis(
        &self,
        token: Address,
        dev_wallet: Option<Address>,
        dev_holding_pct: f64,
        top_holder_pct: f64,
        total_supply: U256,
        market_cap_usd: f64,
        age_minutes: u64,
        reason: String,
    ) -> TokenAnalysis {
        warn!("❌ Token rejected: {}", reason);
        TokenAnalysis {
            token,
            dev_wallet,
            dev_holding_pct,
            top_holder_pct,
            total_supply,
            market_cap_usd,
            age_minutes,
            is_safe: false,
            rejection_reason: Some(reason),
        }
    }
}
