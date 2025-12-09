// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sell execution for closing positions.

use crate::config::Config;
use crate::executor::GasStrategy;
use crate::position::SellDecision;
use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, info};

// Router interface for swaps
sol! {
    #[sol(rpc)]
    interface IRouter {
        function swapExactTokensForETH(
            uint256 amountIn,
            uint256 amountOutMin,
            address[] calldata path,
            address to,
            uint256 deadline
        ) external returns (uint256[] memory amounts);

        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
    }
}

// ERC20 for approval
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256);
    }
}

/// Sell executor for closing positions.
pub struct SellExecutor<P: Provider + Clone> {
    provider: P,
    wallet: EthereumWallet,
    router: Address,
    wmon: Address,
    wallet_address: Address,
    gas_limit: u64,
    gas_strategy: GasStrategy,
    nonce: AtomicU64,
}

impl<P: Provider + Clone> SellExecutor<P> {
    /// Create a new sell executor.
    pub async fn new(
        provider: P,
        wallet: EthereumWallet,
        config: &Config,
    ) -> Result<Self, String> {
        let nonce = provider
            .get_transaction_count(config.wallet_address)
            .await
            .map_err(|e| format!("Failed to get nonce: {}", e))?;

        Ok(Self {
            provider,
            wallet,
            router: config.router_address,
            wmon: config.wmon_address,
            wallet_address: config.wallet_address,
            gas_limit: config.gas_limit,
            gas_strategy: GasStrategy::Normal, // Use normal for sells, not aggressive
            nonce: AtomicU64::new(nonce),
        })
    }

    /// Execute a sell transaction.
    pub async fn sell(
        &self,
        token: Address,
        amount: U256,
        decision: &SellDecision,
    ) -> Result<alloy::primitives::B256, String> {
        info!(
            "🔴 Executing SELL: {:?} - {:?}",
            token, decision
        );

        // Calculate sell amount based on decision
        let sell_amount = match decision {
            SellDecision::SecureProfit { portion, .. } => {
                // Partial sell
                amount * U256::from((*portion * 100.0) as u64) / U256::from(100)
            }
            _ => amount, // Full sell for other cases
        };

        // Get token balance to verify
        let token_contract = IERC20::new(token, &self.provider);
        let balance = token_contract
            .balanceOf(self.wallet_address)
            .call()
            .await
            .map_err(|e| format!("Failed to get balance: {}", e))?;

        let actual_sell_amount = if sell_amount > balance {
            balance
        } else {
            sell_amount
        };

        if actual_sell_amount == U256::ZERO {
            return Err("No tokens to sell".to_string());
        }

        info!("Selling {} tokens", actual_sell_amount);

        // Approve router
        let approve_call = token_contract.approve(self.router, actual_sell_amount);
        let approve_nonce = self.nonce.fetch_add(1, Ordering::SeqCst);

        let approve_tx = TransactionRequest::default()
            .to(token)
            .input(approve_call.calldata().clone().into())
            .nonce(approve_nonce)
            .gas_limit(100_000);

        let pending_approve = self
            .provider
            .send_transaction(approve_tx)
            .await
            .map_err(|e| {
                self.nonce.fetch_sub(1, Ordering::SeqCst);
                format!("Approve failed: {}", e)
            })?;

        let approve_receipt = pending_approve
            .get_receipt()
            .await
            .map_err(|e| format!("Approve receipt failed: {}", e))?;

        if !approve_receipt.status() {
            return Err("Approve transaction failed".to_string());
        }

        info!("✅ Approval confirmed");

        // Get base fee
        let base_fee = self.get_base_fee().await?;
        let (max_fee, priority_fee) = self.gas_strategy.calculate(base_fee);

        // Build swap path: Token -> WMON
        let path = vec![token, self.wmon];

        // Get expected output
        let router = IRouter::new(self.router, &self.provider);
        let amounts_out = router
            .getAmountsOut(actual_sell_amount, path.clone())
            .call()
            .await
            .map_err(|e| format!("getAmountsOut failed: {}", e))?;

        // 5% slippage
        let min_out = amounts_out[1] * U256::from(95) / U256::from(100);
        debug!("Expected MON out: {}, Min: {}", amounts_out[1], min_out);

        // Build swap
        let deadline = U256::from(chrono::Utc::now().timestamp() as u64 + 300);

        let swap_call = router.swapExactTokensForETH(
            actual_sell_amount,
            min_out,
            path,
            self.wallet_address,
            deadline,
        );

        let swap_nonce = self.nonce.fetch_add(1, Ordering::SeqCst);

        let swap_tx = TransactionRequest::default()
            .to(self.router)
            .input(swap_call.calldata().clone().into())
            .nonce(swap_nonce)
            .gas_limit(self.gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        let pending_swap = self
            .provider
            .send_transaction(swap_tx)
            .await
            .map_err(|e| {
                self.nonce.fetch_sub(1, Ordering::SeqCst);
                format!("Swap failed: {}", e)
            })?;

        info!("📤 Sell transaction sent: {:?}", pending_swap.tx_hash());

        let receipt = pending_swap
            .get_receipt()
            .await
            .map_err(|e| format!("Sell receipt failed: {}", e))?;

        if receipt.status() {
            info!(
                "✅ SELL SUCCESS: {:?} - tx: {:?}",
                token, receipt.transaction_hash
            );
        } else {
            error!(
                "❌ SELL FAILED: {:?} - tx: {:?}",
                token, receipt.transaction_hash
            );
        }

        Ok(receipt.transaction_hash)
    }

    async fn get_base_fee(&self) -> Result<u128, String> {
        let block = self
            .provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .map_err(|e| format!("Failed to get block: {}", e))?
            .ok_or("No block found")?;

        block
            .header
            .base_fee_per_gas
            .map(|fee| fee as u128)
            .ok_or("No base fee".to_string())
    }
}
