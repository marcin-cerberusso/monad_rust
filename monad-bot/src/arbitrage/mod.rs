// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arbitrage module for DEX price comparison.

pub mod executor;
pub mod octoswap;
pub mod scanner;
pub mod zkswap;

pub use scanner::{spawn_scanner, ArbitrageOpportunity, DexType, TokenPair};
