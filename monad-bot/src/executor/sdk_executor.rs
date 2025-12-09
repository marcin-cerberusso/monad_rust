// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! SDK-based trade executor using nadfun_sdk Core.

use alloy::primitives::{Address, U256};
use nadfun_sdk::{Core, GasEstimationParams, Network, SlippageUtils};
use nadfun_sdk::types::{BuyParams, SellParams};
use tracing::{error, info, warn};

/// Trade executor using official nad.fun SDK.
pub struct SdkExecutor {
    core: Core,
    slippage_pct: f64,
    gas_buffer_pct: u64,
}

impl SdkExecutor {
    /// Create new SDK executor.
    pub async fn new(
        rpc_url: String,
        private_key: String,
        slippage_pct: f64,
    ) -> Result<Self, String> {
        let core = Core::new(rpc_url, private_key, Network::Mainnet)
            .await
            .map_err(|e| format!("Failed to create Core: {}", e))?;

        info!("✅ SDK Executor initialized: wallet {:?}", core.wallet_address());

        Ok(Self {
            core,
            slippage_pct,
            gas_buffer_pct: 120, // 20% buffer
        })
    }

    /// Get wallet address.
    pub fn wallet_address(&self) -> Address {
        self.core.wallet_address()
    }

    /// Buy tokens on bonding curve.
    pub async fn buy_token(
        &self,
        token: Address,
        amount_mon: U256,
    ) -> Result<String, String> {
        info!(
            "🛒 Buying token {:?} with {} MON (slippage: {}%)",
            token, amount_mon, self.slippage_pct
        );

        // 1. Get quote
        let (router, expected_tokens) = self.core
            .get_amount_out(token, amount_mon, true)
            .await
            .map_err(|e| format!("Failed to get quote: {}", e))?;

        info!("📊 Quote: {} tokens expected", expected_tokens);

        // 2. Apply slippage protection
        let min_tokens = SlippageUtils::calculate_amount_out_min(
            expected_tokens,
            self.slippage_pct,
        );

        info!("🛡️ Min tokens with {}% slippage: {}", self.slippage_pct, min_tokens);

        // 3. Estimate gas
        let gas_params = GasEstimationParams::Buy {
            token,
            amount_in: amount_mon,
            amount_out_min: min_tokens,
            to: self.core.wallet_address(),
            deadline: U256::from(9999999999999999u64),
        };

        let estimated_gas = self.core
            .estimate_gas(&router, gas_params)
            .await
            .map_err(|e| format!("Failed to estimate gas: {}", e))?;

        let gas_with_buffer = estimated_gas * self.gas_buffer_pct / 100;

        info!("⛽ Gas: {} (with {}% buffer)", gas_with_buffer, self.gas_buffer_pct - 100);

        // 4. Execute buy
        let buy_params = BuyParams {
            token,
            amount_in: amount_mon,
            amount_out_min: min_tokens,
            to: self.core.wallet_address(),
            deadline: U256::from(9999999999999999u64),
            gas_limit: Some(gas_with_buffer),
            gas_price: None, // Auto
            nonce: None,     // Auto
        };

        let tx_hash = self.core
            .buy(buy_params, router)
            .await
            .map_err(|e| format!("Buy failed: {}", e))?;

        info!("📤 TX submitted: {}", tx_hash);

        // 5. Wait for receipt
        match self.core.get_receipt(tx_hash).await {
            Ok(receipt) => {
                if receipt.status {
                    info!(
                        "✅ BUY SUCCESS! Token: {:?}, TX: {:?}, Gas: {:?}",
                        token, receipt.transaction_hash, receipt.gas_used
                    );
                    Ok(format!("{:?}", receipt.transaction_hash))
                } else {
                    error!("❌ BUY REVERTED: {:?}", receipt.transaction_hash);
                    Err("Transaction reverted".to_string())
                }
            }
            Err(e) => {
                warn!("⚠️ Receipt not available: {}", e);
                Ok(format!("{}", tx_hash))
            }
        }
    }

    /// Sell tokens on bonding curve.
    pub async fn sell_token(
        &self,
        token: Address,
        amount_tokens: U256,
    ) -> Result<String, String> {
        info!(
            "💰 Selling {} tokens of {:?} (slippage: {}%)",
            amount_tokens, token, self.slippage_pct
        );

        // 1. Get quote (is_buy = false for sell)
        let (router, expected_mon) = self.core
            .get_amount_out(token, amount_tokens, false)
            .await
            .map_err(|e| format!("Failed to get sell quote: {}", e))?;

        info!("📊 Quote: {} MON expected", expected_mon);

        // 2. Apply slippage
        let min_mon = SlippageUtils::calculate_amount_out_min(
            expected_mon,
            self.slippage_pct,
        );

        // 3. Execute sell
        let sell_params = SellParams {
            token,
            amount_in: amount_tokens,
            amount_out_min: min_mon,
            to: self.core.wallet_address(),
            deadline: U256::from(9999999999999999u64),
            gas_limit: None,
            gas_price: None,
            nonce: None,
        };

        let tx_hash = self.core
            .sell(sell_params, router)
            .await
            .map_err(|e| format!("Sell failed: {}", e))?;

        info!("📤 Sell TX submitted: {}", tx_hash);

        Ok(format!("{}", tx_hash))
    }
}
