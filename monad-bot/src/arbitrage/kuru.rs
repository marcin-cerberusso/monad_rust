// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kuru DEX price feed.

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;

/// Kuru Flow Router on Monad Mainnet.
pub const KURU_ROUTER: &str = "0x0d3a1BE29E9dEd63c7a5678b31e847D68F71FFa2";

/// Kuru Flow Entrypoint (Aggregator).
pub const KURU_AGGREGATOR: &str = "0xb3e6778480b2E488385E8205eA05E20060B813cb";

// Kuru uses CLOB, so we query the order book for best bid/ask
sol! {
    #[sol(rpc)]
    interface IKuruRouter {
        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
    }
}

/// Get quote from Kuru for a swap.
pub async fn get_quote<P: Provider + Clone>(
    provider: &P,
    token_in: Address,
    token_out: Address,
    amount_in: U256,
) -> Result<U256, String> {
    let router: Address = KURU_ROUTER.parse().map_err(|e| format!("Invalid address: {}", e))?;
    let contract = IKuruRouter::new(router, provider);
    
    let path = vec![token_in, token_out];
    
    let result = contract
        .getAmountsOut(amount_in, path)
        .call()
        .await
        .map_err(|e| format!("Kuru getAmountsOut failed: {}", e))?;
    
    if result.len() >= 2 {
        Ok(result[1])
    } else {
        Err("Invalid response from Kuru".to_string())
    }
}
