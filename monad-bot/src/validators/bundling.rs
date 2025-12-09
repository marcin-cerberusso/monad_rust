// Copyright (C) 2025 Category Labs, Inc.
#![allow(unused)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bundling detection - identify coordinated wallet manipulation.

use alloy::primitives::Address;
use alloy::providers::Provider;
use std::collections::HashMap;
use tracing::{debug, warn};

/// Bundling analysis result.
#[derive(Debug, Clone)]
pub struct BundlingAnalysis {
    pub token: Address,
    pub is_bundled: bool,
    pub suspicious_wallets: Vec<Address>,
    pub common_funding_source: Option<Address>,
    pub reason: Option<String>,
}

/// Check if token holders show signs of bundling.
pub async fn check_bundling<P: Provider + Clone>(
    provider: &P,
    token: Address,
    top_holders: Vec<Address>,
) -> BundlingAnalysis {
    if top_holders.is_empty() {
        return BundlingAnalysis {
            token,
            is_bundled: false,
            suspicious_wallets: vec![],
            common_funding_source: None,
            reason: None,
        };
    }

    let mut funding_sources: HashMap<Address, Vec<Address>> = HashMap::new();
    let mut suspicious = Vec::new();

    // Check funding source for each holder
    for holder in &top_holders {
        if let Some(source) = get_first_funding_source(provider, *holder).await {
            funding_sources
                .entry(source)
                .or_insert_with(Vec::new)
                .push(*holder);
        }
    }

    // Find common funding sources (3+ wallets from same source = suspicious)
    let mut common_source: Option<Address> = None;
    for (source, wallets) in &funding_sources {
        if wallets.len() >= 3 {
            warn!(
                "🚨 Bundling detected: {} wallets funded from {:?}",
                wallets.len(),
                source
            );
            suspicious.extend(wallets.clone());
            common_source = Some(*source);
        }
    }

    let is_bundled = !suspicious.is_empty();
    let reason = if is_bundled {
        Some(format!(
            "{} wallets share common funding source",
            suspicious.len()
        ))
    } else {
        None
    };

    BundlingAnalysis {
        token,
        is_bundled,
        suspicious_wallets: suspicious,
        common_funding_source: common_source,
        reason,
    }
}

/// Get the first funding source for a wallet.
async fn get_first_funding_source<P: Provider + Clone>(
    provider: &P,
    wallet: Address,
) -> Option<Address> {
    // Get first incoming transaction to this wallet
    // This is a simplified version - full implementation would need transaction history
    
    // For now, we check the wallet's nonce to see if it's a fresh wallet
    match provider.get_transaction_count(wallet).await {
        Ok(nonce) => {
            if nonce == 0 {
                // Fresh wallet with no outgoing txs - suspicious
                debug!("Fresh wallet detected: {:?}", wallet);
            }
            // TODO: Get actual funding source from tx history
            // Would need an indexer or archive node
            None
        }
        Err(_) => None,
    }
}

/// Quick heuristic check for bundling without full tx history.
pub async fn quick_bundling_check<P: Provider + Clone>(
    provider: &P,
    holders: Vec<(Address, u64)>, // (address, balance)
) -> bool {
    // Check for identical balances (sign of coordinated distribution)
    let balances: Vec<u64> = holders.iter().map(|(_, b)| *b).collect();
    
    let mut balance_counts: HashMap<u64, u32> = HashMap::new();
    for bal in &balances {
        *balance_counts.entry(*bal).or_insert(0) += 1;
    }

    // If 3+ wallets have identical balance, suspicious
    for (balance, count) in balance_counts {
        if count >= 3 && balance > 0 {
            warn!(
                "🚨 Suspicious: {} wallets with identical balance {}",
                count, balance
            );
            return true;
        }
    }

    // Check for fresh wallets (nonce = 0)
    let mut fresh_count = 0;
    for (holder, _) in &holders {
        if let Ok(nonce) = provider.get_transaction_count(*holder).await {
            if nonce == 0 {
                fresh_count += 1;
            }
        }
    }

    // If majority of top holders are fresh wallets, suspicious
    if fresh_count > holders.len() / 2 {
        warn!(
            "🚨 Suspicious: {}/{} holders are fresh wallets",
            fresh_count,
            holders.len()
        );
        return true;
    }

    false
}
