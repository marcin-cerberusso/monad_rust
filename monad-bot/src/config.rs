// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Configuration module - loads settings from environment variables.

use alloy::primitives::{Address, U256};
use std::str::FromStr;

/// Main configuration for the sniper bot.
#[derive(Debug, Clone)]
pub struct Config {
    // RPC
    pub rpc_url: String,
    pub ws_url: String,
    pub chain_id: u64,

    // Wallet
    pub private_key: String,
    pub wallet_address: Address,

    // Contracts
    pub router_address: Address,
    pub wmon_address: Address,

    // Sniper settings
    pub auto_snipe_enabled: bool,
    pub snipe_amount_mon: f64,
    pub whale_min_amount: f64,
    pub whale_max_amount: f64,

    // AI Filter
    pub ai_filter_enabled: bool,
    pub ai_min_score: u32,
    pub gemini_api_key: Option<String>,

    // Gas
    pub gas_limit: u64,
    pub priority_fee: u128,
    pub gas_multiplier: f64,

    // Trailing Stop Loss
    pub trailing_drop_pct: f64,
    pub trailing_min_profit: f64,
    pub hard_stop_loss_pct: f64,
    pub secure_profit_pct: f64,
    pub secure_sell_portion: f64,
    pub max_hold_hours: u64,
    pub check_interval_sec: u64,

    // Blacklist
    pub blacklist: Vec<String>,
}

impl Config {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        Ok(Self {
            // RPC
            rpc_url: env_var("MONAD_RPC_URL")?,
            ws_url: env_var("MONAD_WS_URL")?,
            chain_id: env_var_or("CHAIN_ID", "10143").parse().unwrap_or(10143),

            // Wallet
            private_key: env_var("PRIVATE_KEY")?,
            wallet_address: parse_address(&env_var("WALLET_ADDRESS")?)?,

            // Contracts
            router_address: parse_address(&env_var_or(
                "ROUTER_ADDRESS",
                "0x6F6B8F1a20703309951a5127c45B49b1CD981A22",
            ))?,
            wmon_address: parse_address(&env_var_or(
                "WMON_ADDRESS",
                "0x760AfE86e5de5fa0Ee542fc7B7B713e1c5425701",
            ))?,

            // Sniper settings
            auto_snipe_enabled: env_var_or("AUTO_SNIPE_ENABLED", "true")
                .parse()
                .unwrap_or(true),
            snipe_amount_mon: env_var_or("AUTO_SNIPE_AMOUNT_MON", "5.0")
                .parse()
                .unwrap_or(5.0),
            whale_min_amount: env_var_or("WHALE_MIN_AMOUNT_MON", "5.0")
                .parse()
                .unwrap_or(5.0),
            whale_max_amount: env_var_or("WHALE_MAX_AMOUNT_MON", "50.0")
                .parse()
                .unwrap_or(50.0),

            // AI Filter
            ai_filter_enabled: env_var_or("AI_FILTER_ENABLED", "true")
                .parse()
                .unwrap_or(true),
            ai_min_score: env_var_or("AI_MIN_SCORE", "40").parse().unwrap_or(40),
            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),

            // Gas
            gas_limit: env_var_or("AUTO_SNIPE_GAS_LIMIT", "8000000")
                .parse()
                .unwrap_or(8_000_000),
            priority_fee: env_var_or("AUTO_SNIPE_PRIORITY_FEE", "500000000000")
                .parse()
                .unwrap_or(500_000_000_000),
            gas_multiplier: env_var_or("MEMPOOL_GAS_MULTIPLIER", "1.5")
                .parse()
                .unwrap_or(1.5),

            // Trailing Stop Loss
            trailing_drop_pct: env_var_or("TRAILING_DROP_PCT", "20.0")
                .parse()
                .unwrap_or(20.0),
            trailing_min_profit: env_var_or("TRAILING_MIN_PROFIT", "50.0")
                .parse()
                .unwrap_or(50.0),
            hard_stop_loss_pct: env_var_or("HARD_STOP_LOSS_PCT", "-40.0")
                .parse()
                .unwrap_or(-40.0),
            secure_profit_pct: env_var_or("SECURE_PROFIT_PCT", "100.0")
                .parse()
                .unwrap_or(100.0),
            secure_sell_portion: env_var_or("SECURE_SELL_PORTION", "0.3")
                .parse()
                .unwrap_or(0.3),
            max_hold_hours: env_var_or("MAX_HOLD_HOURS", "48")
                .parse()
                .unwrap_or(48),
            check_interval_sec: env_var_or("CHECK_INTERVAL_SEC", "5")
                .parse()
                .unwrap_or(5),

            // Blacklist
            blacklist: env_var_or("AUTO_SNIPE_BLACKLIST", "test,scam,rug,honeypot,fake")
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect(),
        })
    }

    /// Convert MON amount to wei (18 decimals).
    pub fn mon_to_wei(&self, mon: f64) -> U256 {
        let wei = (mon * 1e18) as u128;
        U256::from(wei)
    }
}

fn env_var(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{} not set", name))
}

fn env_var_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_address(s: &str) -> Result<Address, String> {
    Address::from_str(s).map_err(|e| format!("Invalid address {}: {}", s, e))
}
