// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arbitrage opportunity scanner for Monad DEXs.
//! Compares prices between ZKSwap and OctoSwap.

use super::{octoswap, zkswap};
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Minimum profit threshold (0.3% = 30 bps).
const MIN_PROFIT_BPS: u64 = 30;

/// Arbitrage opportunity detected.
#[derive(Debug, Clone)]
pub struct ArbitrageOpportunity {
    pub token_a: Address,
    pub token_b: Address,
    pub amount_in: U256,
    pub buy_on: DexType,
    pub sell_on: DexType,
    pub expected_profit: U256,
    pub profit_bps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DexType {
    ZKSwap,
    OctoSwap,
}

impl std::fmt::Display for DexType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DexType::ZKSwap => write!(f, "ZKSwap"),
            DexType::OctoSwap => write!(f, "OctoSwap"),
        }
    }
}

/// Token pair to monitor.
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub token_a: Address,
    pub token_b: Address,
    pub name: String,
}

/// Arbitrage scanner that compares prices across DEXs.
pub struct ArbitrageScanner<P: Provider + Clone> {
    provider: P,
    pairs: Vec<TokenPair>,
    scan_amount: U256,
    min_profit_bps: u64,
}

impl<P: Provider + Clone + Send + Sync + 'static> ArbitrageScanner<P> {
    pub fn new(provider: P, pairs: Vec<TokenPair>, scan_amount: U256) -> Self {
        Self {
            provider,
            pairs,
            scan_amount,
            min_profit_bps: MIN_PROFIT_BPS,
        }
    }

    /// Scan all pairs for arbitrage opportunities.
    pub async fn scan(&self) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();

        for pair in &self.pairs {
            match self.check_pair(pair).await {
                Ok(Some(opp)) => {
                    info!(
                        "💰 ARB FOUND: {} - Buy on {}, Sell on {} - Profit: {} bps",
                        pair.name, opp.buy_on, opp.sell_on, opp.profit_bps
                    );
                    opportunities.push(opp);
                }
                Ok(None) => {
                    debug!("No arb for {}", pair.name);
                }
                Err(e) => {
                    debug!("Check {} failed: {}", pair.name, e);
                }
            }
        }

        opportunities
    }

    async fn check_pair(&self, pair: &TokenPair) -> Result<Option<ArbitrageOpportunity>, String> {
        // Get quotes from both DEXs
        let (zkswap_quote, octo_quote) = tokio::join!(
            zkswap::get_quote(&self.provider, pair.token_a, pair.token_b, self.scan_amount),
            octoswap::get_quote(&self.provider, pair.token_a, pair.token_b, self.scan_amount)
        );

        let zkswap_out = zkswap_quote?;
        let octo_out = octo_quote?;

        debug!(
            "{}: ZKSwap={}, OctoSwap={}",
            pair.name, zkswap_out, octo_out
        );

        // Check if there's profitable arbitrage
        if zkswap_out > octo_out {
            // Buy on OctoSwap (cheaper), sell on ZKSwap (more expensive)
            let profit = zkswap_out - octo_out;
            let profit_bps = (profit * U256::from(10000) / octo_out).to::<u64>();

            if profit_bps >= self.min_profit_bps {
                return Ok(Some(ArbitrageOpportunity {
                    token_a: pair.token_a,
                    token_b: pair.token_b,
                    amount_in: self.scan_amount,
                    buy_on: DexType::OctoSwap,
                    sell_on: DexType::ZKSwap,
                    expected_profit: profit,
                    profit_bps,
                }));
            }
        } else if octo_out > zkswap_out {
            // Buy on ZKSwap (cheaper), sell on OctoSwap (more expensive)
            let profit = octo_out - zkswap_out;
            let profit_bps = (profit * U256::from(10000) / zkswap_out).to::<u64>();

            if profit_bps >= self.min_profit_bps {
                return Ok(Some(ArbitrageOpportunity {
                    token_a: pair.token_a,
                    token_b: pair.token_b,
                    amount_in: self.scan_amount,
                    buy_on: DexType::ZKSwap,
                    sell_on: DexType::OctoSwap,
                    expected_profit: profit,
                    profit_bps,
                }));
            }
        }

        Ok(None)
    }
}

/// Spawn scanner as background task.
pub fn spawn_scanner<P: Provider + Clone + Send + Sync + 'static>(
    provider: P,
    pairs: Vec<TokenPair>,
    scan_amount: U256,
    interval_ms: u64,
    tx: mpsc::Sender<ArbitrageOpportunity>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let scanner = ArbitrageScanner::new(provider, pairs, scan_amount);
        
        info!("🔍 Arbitrage scanner started (ZKSwap ↔ OctoSwap, {}ms interval)", interval_ms);

        loop {
            let opportunities = scanner.scan().await;

            for opp in opportunities {
                if let Err(e) = tx.send(opp).await {
                    warn!("Failed to send opportunity: {}", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;
        }
    })
}
