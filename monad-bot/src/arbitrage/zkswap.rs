// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! ZKSwap DEX price feed for Monad.
//! Uses RouterV2 at 0x68225b5ba7cE309fD0d3f0C9A74b947c7d7e03dA

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;

/// ZKSwap Router V2 on Monad Mainnet.
pub const ZKSWAP_ROUTER: &str = "0x68225b5ba7cE309fD0d3f0C9A74b947c7d7e03dA";

sol! {
    #[sol(rpc)]
    interface IZKSwapRouter {
        function getAmountsOut(uint amountIn, address[] memory path) external view returns (uint[] memory amounts);
    }
}

/// Get quote from ZKSwap for a token pair.
pub async fn get_quote<P: Provider + Clone>(
    provider: &P,
    token_in: Address,
    token_out: Address,
    amount_in: U256,
) -> Result<U256, String> {
    let router: Address = ZKSWAP_ROUTER.parse().map_err(|e| format!("Invalid router address: {}", e))?;
    let contract = IZKSwapRouter::new(router, provider);

    let path = vec![token_in, token_out];

    match contract.getAmountsOut(amount_in, path).call().await {
        Ok(amounts) => {
            if amounts.len() >= 2 {
                Ok(amounts[1])
            } else {
                Err("Invalid amounts returned".to_string())
            }
        }
        Err(e) => Err(format!("ZKSwap quote failed: {}", e)),
    }
}
