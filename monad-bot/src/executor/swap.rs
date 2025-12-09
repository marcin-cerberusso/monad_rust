// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Swap execution for buying tokens.

use crate::config::Config;
use crate::executor::GasStrategy;
use crate::strategies::BuyDecision;
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
        function swapExactETHForTokens(
            uint256 amountOutMin,
            address[] calldata path,
            address to,
            uint256 deadline
        ) external payable returns (uint256[] memory amounts);

        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
    }
}

/// Swap executor for buying tokens.
pub struct SwapExecutor<P: Provider + Clone> {
    provider: P,
    wallet: EthereumWallet,
    router: Address,
    wmon: Address,
    wallet_address: Address,
    gas_limit: u64,
    gas_strategy: GasStrategy,
    nonce: AtomicU64,
}

impl<P: Provider + Clone> SwapExecutor<P> {
    /// Create a new swap executor.
    pub async fn new(
        provider: P,
        wallet: EthereumWallet,
        config: &Config,
    ) -> Result<Self, String> {
        // Get current nonce
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
            gas_strategy: GasStrategy::from_multiplier(config.gas_multiplier),
            nonce: AtomicU64::new(nonce),
        })
    }

    /// Execute a buy transaction.
    pub async fn buy(&self, decision: &BuyDecision) -> Result<alloy::primitives::B256, String> {
        info!(
            "🚀 Executing BUY: {} ({}) for {} wei",
            decision.name, decision.symbol, decision.amount_wei
        );

        // Get current base fee
        let base_fee = self.get_base_fee().await?;
        let (max_fee, priority_fee) = self.gas_strategy.calculate(base_fee);

        debug!(
            "Gas: base_fee={}, max_fee={}, priority={}",
            base_fee, max_fee, priority_fee
        );

        // Build swap path: WMON -> Token
        let path = vec![self.wmon, decision.token];

        // Get expected output (for slippage calculation)
        let router = IRouter::new(self.router, &self.provider);
        let amounts_out = router
            .getAmountsOut(decision.amount_wei, path.clone())
            .call()
            .await
            .map_err(|e| format!("getAmountsOut failed: {}", e))?;

        // 5% slippage tolerance
        let amounts = amounts_out;
        let min_out = amounts[1] * U256::from(95) / U256::from(100);
        debug!("Expected out: {}, Min out (5% slippage): {}", amounts[1], min_out);

        // Build swap calldata
        let deadline = U256::from(chrono::Utc::now().timestamp() as u64 + 300); // 5 min deadline

        let call = router.swapExactETHForTokens(
            min_out,
            path,
            self.wallet_address,
            deadline,
        );

        // Get nonce
        let nonce = self.nonce.fetch_add(1, Ordering::SeqCst);
        debug!("Using nonce: {}", nonce);

        // Build transaction
        let tx = TransactionRequest::default()
            .to(self.router)
            .value(decision.amount_wei)
            .input(call.calldata().clone().into())
            .nonce(nonce)
            .gas_limit(self.gas_limit)
            .max_fee_per_gas(max_fee)
            .max_priority_fee_per_gas(priority_fee);

        // Send transaction
        let pending = self
            .provider
            .send_transaction(tx)
            .await
            .map_err(|e| {
                // Rollback nonce on failure
                self.nonce.fetch_sub(1, Ordering::SeqCst);
                format!("Failed to send tx: {}", e)
            })?;

        info!("📤 Transaction sent: {:?}", pending.tx_hash());

        // Wait for receipt
        let receipt = pending
            .get_receipt()
            .await
            .map_err(|e| format!("Failed to get receipt: {}", e))?;

        if receipt.status() {
            info!(
                "✅ BUY SUCCESS: {} ({}) - tx: {:?}",
                decision.name, decision.symbol, receipt.transaction_hash
            );
        } else {
            error!(
                "❌ BUY FAILED: {} ({}) - tx: {:?}",
                decision.name, decision.symbol, receipt.transaction_hash
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
