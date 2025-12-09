// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! DEX price feeds for arbitrage detection.

pub mod executor;
pub mod kuru;
pub mod octoswap;
pub mod scanner;

pub use executor::ArbitrageExecutor;
pub use scanner::{ArbitrageOpportunity, ArbitrageScanner, DexType, TokenPair, spawn_scanner};
