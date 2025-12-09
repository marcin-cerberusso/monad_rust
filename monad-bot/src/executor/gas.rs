// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gas strategy for transaction priority.

/// Gas strategy determines how aggressively we bid for transaction inclusion.
#[derive(Debug, Clone, Copy)]
pub enum GasStrategy {
    /// Normal priority - good for non-time-sensitive transactions.
    /// base_fee * 1.1 + 1 gwei priority
    Normal,

    /// Aggressive priority - for sniper bot.
    /// base_fee * 1.5 + 10 gwei priority
    Aggressive,

    /// Maximum priority - for frontrunning.
    /// base_fee * 2.0 + max priority (500 gwei)
    Frontrun,
}

impl GasStrategy {
    /// Calculate max fee per gas and priority fee.
    ///
    /// Returns (max_fee_per_gas, max_priority_fee_per_gas) in wei.
    pub fn calculate(&self, base_fee: u128) -> (u128, u128) {
        match self {
            Self::Normal => {
                let max_fee = base_fee * 110 / 100; // 1.1x
                let priority = 1_000_000_000; // 1 gwei
                (max_fee + priority, priority)
            }
            Self::Aggressive => {
                let max_fee = base_fee * 150 / 100; // 1.5x
                let priority = 10_000_000_000; // 10 gwei
                (max_fee + priority, priority)
            }
            Self::Frontrun => {
                let max_fee = base_fee * 200 / 100; // 2x
                let priority = 500_000_000_000; // 500 gwei
                (max_fee + priority, priority)
            }
        }
    }

    /// Get strategy from config multiplier.
    pub fn from_multiplier(multiplier: f64) -> Self {
        if multiplier >= 2.0 {
            Self::Frontrun
        } else if multiplier >= 1.5 {
            Self::Aggressive
        } else {
            Self::Normal
        }
    }
}

impl Default for GasStrategy {
    fn default() -> Self {
        Self::Aggressive
    }
}
