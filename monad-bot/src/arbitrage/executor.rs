// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arbitrage executor using FlashArbitrage contract.

use crate::arbitrage::{ArbitrageOpportunity, DexType};
use crate::config::Config;
use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{error, info};

// FlashArbitrage contract interface
sol! {
    #[sol(rpc)]
    interface IFlashArbitrage {
        function executeArbitrage(
            address pairA,
            address pairB,
            address tokenBorrow,
            uint256 amountBorrow,
            bool borrowFromA
        ) external;
    }
}

/// Router addresses for each DEX.
fn get_router(dex: DexType) -> Address {
    match dex {
        DexType::Kuru => "0x0d3a1BE29E9dEd63c7a5678b31e847D68F71FFa2".parse().unwrap(),
        DexType::OctoSwap => "0x60fd5Aa15Debd5ffdEfB5129FD9FD8A34d80d608".parse().unwrap(),
    }
}

/// Arbitrage executor.
pub struct ArbitrageExecutor<P: Provider + Clone> {
    provider: P,
    flash_contract: Address,
    nonce: AtomicU64,
    gas_limit: u64,
}

impl<P: Provider + Clone> ArbitrageExecutor<P> {
    pub async fn new(provider: P, config: &Config) -> Result<Self, String> {
        let nonce = provider
            .get_transaction_count(config.wallet_address)
            .await
            .map_err(|e| format!("Failed to get nonce: {}", e))?;

        // Use arbitrage contract address from config or default
        let flash_contract = config.arbitrage_contract
            .unwrap_or_else(|| Address::ZERO);

        Ok(Self {
            provider,
            flash_contract,
            nonce: AtomicU64::new(nonce),
            gas_limit: config.gas_limit,
        })
    }

    /// Execute arbitrage opportunity.
    pub async fn execute(&self, opp: &ArbitrageOpportunity) -> Result<(), String> {
        if self.flash_contract == Address::ZERO {
            return Err("FlashArbitrage contract not deployed".to_string());
        }

        info!(
            "⚡ Executing arbitrage: {} -> {} on {} -> {}",
            opp.token_a, opp.token_b, opp.buy_on, opp.sell_on
        );

        let contract = IFlashArbitrage::new(self.flash_contract, &self.provider);

        // Determine which pair to borrow from
        let borrow_from_a = opp.buy_on == DexType::OctoSwap;

        let call = contract.executeArbitrage(
            get_router(DexType::OctoSwap), // pairA
            get_router(DexType::Kuru),      // pairB
            opp.token_a,
            opp.amount_in,
            borrow_from_a,
        );

        let nonce = self.nonce.fetch_add(1, Ordering::SeqCst);

        let tx = TransactionRequest::default()
            .to(self.flash_contract)
            .input(call.calldata().clone().into())
            .nonce(nonce)
            .gas_limit(self.gas_limit);

        match self.provider.send_transaction(tx).await {
            Ok(pending) => {
                info!("📤 Arb TX sent: {:?}", pending.tx_hash());

                match pending.get_receipt().await {
                    Ok(receipt) => {
                        if receipt.status() {
                            info!(
                                "✅ ARB SUCCESS! Profit: {} bps, TX: {:?}",
                                opp.profit_bps, receipt.transaction_hash
                            );
                        } else {
                            error!(
                                "❌ ARB REVERTED (no profit): {:?}",
                                receipt.transaction_hash
                            );
                        }
                    }
                    Err(e) => error!("Failed to get receipt: {}", e),
                }
            }
            Err(e) => {
                self.nonce.fetch_sub(1, Ordering::SeqCst);
                error!("Failed to send arb TX: {}", e);
            }
        }

        Ok(())
    }
}
