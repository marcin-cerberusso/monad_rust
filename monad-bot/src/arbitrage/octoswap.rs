// Copyright (C) 2025 Category Labs, Inc.
#![allow(unused)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! OctoSwap DEX price feed.

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;

/// OctoSwap Classic Router on Monad Mainnet.
pub const OCTO_ROUTER_CLASSIC: &str = "0x60fd5Aa15Debd5ffdEfB5129FD9FD8A34d80d608";

/// OctoSwap Universal Router.
pub const OCTO_ROUTER_UNIVERSAL: &str = "0x241BF19641839b249E8174Bd22FACd3d3ac0642A";

/// OctoSwap Factory Classic.
pub const OCTO_FACTORY: &str = "0xCe104732685B9D7b2F07A09d828F6b19786cdA32";

// Standard Uniswap V2 interface (OctoSwap is V2 compatible)
sol! {
    #[sol(rpc)]
    interface IOctoRouter {
        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
            
        function factory() external view returns (address);
    }
}

/// Get quote from OctoSwap for a swap.
pub async fn get_quote<P: Provider + Clone>(
    provider: &P,
    token_in: Address,
    token_out: Address,
    amount_in: U256,
) -> Result<U256, String> {
    let router: Address = OCTO_ROUTER_CLASSIC.parse().map_err(|e| format!("Invalid address: {}", e))?;
    let contract = IOctoRouter::new(router, provider);
    
    let path = vec![token_in, token_out];
    
    let result = contract
        .getAmountsOut(amount_in, path)
        .call()
        .await
        .map_err(|e| format!("OctoSwap getAmountsOut failed: {}", e))?;
    
    if result.len() >= 2 {
        Ok(result[1])
    } else {
        Err("Invalid response from OctoSwap".to_string())
    }
}
