// Copyright (C) 2025 Category Labs, Inc.
#![allow(unused)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arbitrage executor for sending flash swap transactions.

use alloy::{
    primitives::{Address, U256},
    providers::Provider,
    sol,
};

// Generate bindings for FlashArbitrage contract
sol! {
    #[sol(rpc)]
    interface IFlashArbitrage {
        function executeArbitrage(
            address pairA,
            address pairB,
            address tokenIn,
            uint256 amountIn
        ) external;

        function withdraw(address token) external;
    }
}

/// Executor for atomic arbitrage transactions.
pub struct ArbitrageExecutor<P: Provider + Clone> {
    provider: P,
    contract_address: Address,
}

impl<P: Provider + Clone> ArbitrageExecutor<P> {
    /// Create a new executor with provider and contract address.
    pub fn new(provider: P, contract_address: Address) -> Self {
        Self {
            provider,
            contract_address,
        }
    }

    /// Execute atomic arbitrage between two Uniswap V2 pairs.
    ///
    /// # Arguments
    /// * `pair_a` - First pair to flash swap from
    /// * `pair_b` - Second pair to swap on for profit
    /// * `token_in` - Token to borrow and trade
    /// * `amount_in` - Amount to borrow
    ///
    /// # Returns
    /// Transaction hash on success.
    pub async fn execute_arbitrage(
        &self,
        pair_a: Address,
        pair_b: Address,
        token_in: Address,
        amount_in: U256,
    ) -> Result<alloy::primitives::B256, String> {
        let contract = IFlashArbitrage::new(self.contract_address, &self.provider);

        let call = contract.executeArbitrage(pair_a, pair_b, token_in, amount_in);

        let pending = call
            .send()
            .await
            .map_err(|e| format!("Failed to send tx: {e}"))?;

        let receipt = pending
            .get_receipt()
            .await
            .map_err(|e| format!("Failed to get receipt: {e}"))?;

        Ok(receipt.transaction_hash)
    }

    /// Withdraw tokens from the contract (owner only).
    pub async fn withdraw(
        &self,
        token: Address,
    ) -> Result<alloy::primitives::B256, String> {
        let contract = IFlashArbitrage::new(self.contract_address, &self.provider);

        let call = contract.withdraw(token);

        let pending = call
            .send()
            .await
            .map_err(|e| format!("Failed to send withdraw tx: {e}"))?;

        let receipt = pending
            .get_receipt()
            .await
            .map_err(|e| format!("Failed to get receipt: {e}"))?;

        Ok(receipt.transaction_hash)
    }
}
