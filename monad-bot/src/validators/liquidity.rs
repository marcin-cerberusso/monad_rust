// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Liquidity validation.

use alloy::primitives::U256;
use tracing::debug;

/// Minimum liquidity in MON (wei) required for snipe.
/// 10 MON = 10 * 10^18
const MIN_LIQUIDITY_WEI: u128 = 10_000_000_000_000_000_000;

/// Check if token has sufficient liquidity.
pub fn check_liquidity(initial_liquidity: Option<U256>, min_liquidity_wei: Option<u128>) -> bool {
    let min = min_liquidity_wei.unwrap_or(MIN_LIQUIDITY_WEI);

    match initial_liquidity {
        Some(liquidity) => {
            let is_sufficient = liquidity >= U256::from(min);
            debug!(
                "Liquidity check: {} >= {} = {}",
                liquidity, min, is_sufficient
            );
            is_sufficient
        }
        None => {
            // If no liquidity info, assume it's OK (will be validated on-chain)
            debug!("No liquidity info, assuming sufficient");
            true
        }
    }
}

/// Convert MON to wei for comparison.
pub fn mon_to_wei(mon: f64) -> u128 {
    (mon * 1e18) as u128
}
