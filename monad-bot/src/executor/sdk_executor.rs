// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! SDK-based trade executor using nadfun_sdk Core.
//! Based on official buy.rs example from SDK.

use alloy::eips::BlockId;
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;
use nadfun_sdk::{Core, GasEstimationParams, Network, SlippageUtils};
use nadfun_sdk::types::{BuyParams, GasPricing, SellParams, Router};
use tracing::{error, info, warn};

// ERC20 interface for balance, approval, and token info
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function name() external view returns (string);
        function symbol() external view returns (string);
    }
}

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

    /// Buy tokens with custom gas price (for front-running).
    pub async fn buy_token_with_gas(
        &self,
        token: Address,
        amount_mon: U256,
        priority_gas_price: u128,
    ) -> Result<String, String> {
        let wallet = self.core.wallet_address();
        
        info!(
            "🚀 FRONT-RUNNING {:?} with {} MON (Gas: {} wei)",
            token, amount_mon, priority_gas_price
        );

        // Get quote and router
        let (router, _) = self.core
            .get_amount_out(token, amount_mon, true)
            .await
            .map_err(|e| format!("Failed to get router: {}", e))?;

        // 1. Get nonce
        let current_nonce = self.core.provider()
            .get_transaction_count(wallet)
            .block_id(BlockId::latest())
            .await
            .map_err(|e| format!("Failed to get nonce: {}", e))?;

        // 2. Execute buy with explicit gas price
        let deadline = U256::from(9999999999999999u64);
        let buy_params = BuyParams {
            token,
            amount_in: amount_mon,
            amount_out_min: U256::ZERO, // Accept high slippage for sniping
            to: wallet,
            deadline,
            gas_limit: Some(8_000_000), 
            gas_price: Some(GasPricing::LegacyWithPrice {
                gas_price: priority_gas_price,
            }),
            nonce: Some(current_nonce),
        };

        let tx_hash = self.core
            .buy(buy_params, router)
            .await
            .map_err(|e| format!("Front-run failed: {}", e))?;

        info!("🔫 Front-run TX sent: {}", tx_hash);
        Ok(format!("{}", tx_hash))
    }

    /// Sell tokens on bonding curve with automatic approve.
    /// Uses 15% slippage for sells (more volatile than buys).
    pub async fn sell_token(
        &self,
        token: Address,
        amount_tokens: U256,
    ) -> Result<String, String> {
        let wallet = self.core.wallet_address();
        
        // Use higher slippage for sells (15%) - bonding curve tokens are volatile
        let sell_slippage = 15.0;
        
        info!(
            "💰 Selling {} tokens of {:?} (slippage: {}%)",
            amount_tokens, token, sell_slippage
        );

        // 1. Get quote to find out which router to use
        let (router, expected_mon) = self.core
            .get_amount_out(token, amount_tokens, false)
            .await
            .map_err(|e| format!("Failed to get sell quote: {}", e))?;

        info!("📊 Quote: {} MON expected via {:?}", expected_mon, router);

        // 2. Get router address for approval
        let router_address = router.address();

        // 3. Check current allowance and approve if needed
        let token_contract = IERC20::new(token, self.core.provider());
        
        let current_allowance = token_contract
            .allowance(wallet, router_address)
            .call()
            .await
            .map_err(|e| format!("Failed to check allowance: {}", e))?;

        if current_allowance < amount_tokens {
            info!("🔐 Approving {} tokens for router {:?}", amount_tokens, router_address);
            
            // Approve max amount to avoid future approvals
            let max_approval = U256::MAX;
            
            let approve_tx = token_contract
                .approve(router_address, max_approval);
            
            let pending = self.core.provider()
                .send_transaction(
                    alloy::rpc::types::TransactionRequest::default()
                        .to(token)
                        .input(approve_tx.calldata().clone().into())
                )
                .await
                .map_err(|e| format!("Approve TX failed: {}", e))?;
            
            info!("📤 Approve TX submitted: {:?}", pending.tx_hash());
            
            // Wait for approval confirmation
            let receipt = pending
                .get_receipt()
                .await
                .map_err(|e| format!("Approve receipt failed: {}", e))?;
            
            if !receipt.status() {
                return Err("Approve transaction reverted".to_string());
            }
            
            info!("✅ Approval confirmed");
        } else {
            info!("✅ Already approved for router");
        }

        // 4. Apply higher slippage for sells
        let min_mon = SlippageUtils::calculate_amount_out_min(
            expected_mon,
            sell_slippage,
        );
        
        info!("🛡️ Min MON with {}% slippage: {}", sell_slippage, min_mon);

        // 5. Execute sell
        let sell_params = SellParams {
            token,
            amount_in: amount_tokens,
            amount_out_min: min_mon,
            to: wallet,
            deadline: U256::from(9999999999999999u64),
            gas_limit: Some(500000), // Explicit gas limit
            gas_price: None,
            nonce: None,
        };

        let tx_hash = self.core
            .sell(sell_params, router)
            .await
            .map_err(|e| format!("Sell failed: {}", e))?;

        info!("📤 Sell TX submitted: {}", tx_hash);

        // 6. Wait for receipt
        match self.core.get_receipt(tx_hash).await {
            Ok(receipt) => {
                if receipt.status {
                    info!(
                        "✅ SELL SUCCESS! TX: {:?}, Gas: {:?}",
                        receipt.transaction_hash, receipt.gas_used
                    );
                    Ok(format!("{:?}", receipt.transaction_hash))
                } else {
                    error!("❌ SELL REVERTED: {:?}", receipt.transaction_hash);
                    Err("Sell transaction reverted".to_string())
                }
            }
            Err(e) => {
                warn!("⚠️ Receipt not available: {}", e);
                Ok(format!("{}", tx_hash))
            }
        }
    }

    /// Sell tokens with custom slippage (for retries with higher tolerance).
    pub async fn sell_token_with_slippage(
        &self,
        token: Address,
        amount_tokens: U256,
        slippage_pct: f64,
    ) -> Result<String, String> {
        let wallet = self.core.wallet_address();
        
        info!(
            "💰 Selling {} tokens of {:?} (custom slippage: {}%)",
            amount_tokens, token, slippage_pct
        );

        // Get quote
        let (router, expected_mon) = self.core
            .get_amount_out(token, amount_tokens, false)
            .await
            .map_err(|e| format!("Failed to get sell quote: {}", e))?;

        info!("📊 Quote: {} MON expected via {:?}", expected_mon, router);

        // Already approved from previous attempt, skip approval check
        
        // Apply custom slippage
        let min_mon = SlippageUtils::calculate_amount_out_min(expected_mon, slippage_pct);
        
        info!("🛡️ Min MON with {}% slippage: {}", slippage_pct, min_mon);

        // Execute sell
        let sell_params = SellParams {
            token,
            amount_in: amount_tokens,
            amount_out_min: min_mon,
            to: wallet,
            deadline: U256::from(9999999999999999u64),
            gas_limit: Some(500000),
            gas_price: None,
            nonce: None,
        };

        let tx_hash = self.core
            .sell(sell_params, router)
            .await
            .map_err(|e| format!("Sell failed: {}", e))?;

        info!("📤 Sell TX submitted: {}", tx_hash);

        // Wait for receipt
        match self.core.get_receipt(tx_hash).await {
            Ok(receipt) => {
                if receipt.status {
                    info!("✅ SELL SUCCESS! TX: {:?}", receipt.transaction_hash);
                    Ok(format!("{:?}", receipt.transaction_hash))
                } else {
                    Err("Sell transaction reverted".to_string())
                }
            }
            Err(e) => {
                warn!("⚠️ Receipt not available: {}", e);
                Ok(format!("{}", tx_hash))
            }
        }
    }

    /// Get token price in MON using SDK (for bonding curve tokens).
    /// Returns the amount of MON you would receive for selling `amount_tokens`.
    pub async fn get_token_price_mon(
        &self,
        token: Address,
        amount_tokens: U256,
    ) -> Result<f64, String> {
        // Use SDK's get_amount_out with is_buy=false to get sell quote
        let (_router, expected_mon) = self.core
            .get_amount_out(token, amount_tokens, false)
            .await
            .map_err(|e| format!("Failed to get price: {}", e))?;

        // Convert wei to MON
        let mon = expected_mon.to::<u128>() as f64 / 1e18;
        Ok(mon)
    }

    /// Check if token has graduated from bonding curve.
    pub async fn is_graduated(&self, token: Address) -> Result<bool, String> {
        self.core
            .is_graduated(token)
            .await
            .map_err(|e| format!("Failed to check graduated: {}", e))
    }

    /// Get a reference to the SDK Core for direct access.
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Get token balance for wallet using ERC20 interface.
    pub async fn get_token_balance(&self, token: Address) -> Result<U256, String> {
        let wallet = self.core.wallet_address();
        let token_contract = IERC20::new(token, self.core.provider());
        
        token_contract
            .balanceOf(wallet)
            .call()
            .await
            .map_err(|e| format!("Failed to get balance: {}", e))
    }

    /// Get token name and symbol from chain.
    pub async fn get_token_info(&self, token: Address) -> Result<(String, String), String> {
        let token_contract = IERC20::new(token, self.core.provider());
        
        let name = token_contract
            .name()
            .call()
            .await
            .map_or_else(|_| "Unknown".to_string(), |s| s.to_string());
        
        let symbol = token_contract
            .symbol()
            .call()
            .await
            .map_or_else(|_| "???".to_string(), |s| s.to_string());
        
        Ok((name, symbol))
    }
}
