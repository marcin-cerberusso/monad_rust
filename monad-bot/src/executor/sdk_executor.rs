// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! SDK-based trade executor using nadfun_sdk Core.
//! Based on official buy.rs example from SDK.

use alloy::eips::BlockId;
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use nadfun_sdk::{Core, GasEstimationParams, Network, SlippageUtils};
use nadfun_sdk::types::{BuyParams, GasPricing, SellParams};
use tracing::{error, info, warn};

/// Trade executor using official nad.fun SDK.
pub struct SdkExecutor {
    core: Core,
    slippage_pct: f64,
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
        })
    }

    /// Get wallet address.
    pub fn wallet_address(&self) -> Address {
        self.core.wallet_address()
    }

    /// Buy tokens on bonding curve (official SDK method).
    pub async fn buy_token(
        &self,
        token: Address,
        amount_mon: U256,
    ) -> Result<String, String> {
        let wallet = self.core.wallet_address();
        
        info!(
            "🛒 Buying token {:?} with {} MON (slippage: {}%)",
            token, amount_mon, self.slippage_pct
        );

        // 1. Check token status before buying
        let is_locked = self.core.is_locked(token).await
            .map_err(|e| format!("Failed to check locked: {}", e))?;
        let is_graduated = self.core.is_graduated(token).await
            .map_err(|e| format!("Failed to check graduated: {}", e))?;
        
        if is_locked {
            warn!("⚠️ Token is locked!");
        }
        
        info!("📊 Token status: locked={}, graduated={}", is_locked, is_graduated);

        // 2. Get quote
        let (router, expected_tokens) = self.core
            .get_amount_out(token, amount_mon, true)
            .await
            .map_err(|e| format!("Failed to get quote: {}", e))?;

        if expected_tokens == U256::ZERO {
            return Err("Invalid quote: amount_out is zero".to_string());
        }

        info!("📊 Quote: {} tokens expected via {:?}", expected_tokens, router);

        // 3. Apply slippage protection
        let amount_out_min = SlippageUtils::calculate_amount_out_min(
            expected_tokens,
            self.slippage_pct,
        );

        info!("🛡️ Min tokens with {}% slippage: {}", self.slippage_pct, amount_out_min);

        // 4. Get nonce
        let current_nonce = self.core.provider()
            .get_transaction_count(wallet)
            .block_id(BlockId::latest())
            .await
            .map_err(|e| format!("Failed to get nonce: {}", e))?;

        // 5. Get gas price
        let network_gas_price = self.core.provider()
            .get_gas_price()
            .await
            .map_err(|e| format!("Failed to get gas price: {}", e))?;
        let recommended_gas_price = (network_gas_price * 300) / 100; // 3x network price

        // 6. Estimate gas
        let deadline = U256::from(9999999999999999u64);
        let gas_params = GasEstimationParams::Buy {
            token,
            amount_in: amount_mon,
            amount_out_min,
            to: wallet,
            deadline,
        };

        let estimated_gas = match self.core.estimate_gas(&router, gas_params).await {
            Ok(gas) => {
                info!("⛽ Estimated gas: {}", gas);
                gas
            }
            Err(e) => {
                warn!("⚠️ Gas estimation failed: {}, using fallback", e);
                300000
            }
        };

        let gas_with_buffer = estimated_gas * 120 / 100;

        // 7. Execute buy
        let buy_params = BuyParams {
            token,
            amount_in: amount_mon,
            amount_out_min,
            to: wallet,
            deadline,
            gas_limit: Some(gas_with_buffer),
            gas_price: Some(GasPricing::LegacyWithPrice {
                gas_price: recommended_gas_price,
            }),
            nonce: Some(current_nonce),
        };

        let tx_hash = self.core
            .buy(buy_params, router)
            .await
            .map_err(|e| format!("Buy failed: {}", e))?;

        info!("📤 TX submitted: {}", tx_hash);

        // 8. Wait for receipt
        match self.core.get_receipt(tx_hash).await {
            Ok(receipt) => {
                if receipt.status {
                    info!(
                        "✅ BUY SUCCESS! TX: {:?}, Gas: {:?}",
                        receipt.transaction_hash, receipt.gas_used
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
        let wallet = self.core.wallet_address();
        
        info!(
            "💰 Selling {} tokens of {:?} (slippage: {}%)",
            amount_tokens, token, self.slippage_pct
        );

        // Get quote (is_buy = false for sell)
        let (router, expected_mon) = self.core
            .get_amount_out(token, amount_tokens, false)
            .await
            .map_err(|e| format!("Failed to get sell quote: {}", e))?;

        info!("📊 Quote: {} MON expected", expected_mon);

        // Apply slippage
        let min_mon = SlippageUtils::calculate_amount_out_min(
            expected_mon,
            self.slippage_pct,
        );

        // Execute sell
        let sell_params = SellParams {
            token,
            amount_in: amount_tokens,
            amount_out_min: min_mon,
            to: wallet,
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
