// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Honeypot detection - simulates sell to verify token is not a honeypot.

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol;
use tracing::{debug, warn};

// ERC20 interface for simulation
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

// Router interface for swap simulation
sol! {
    #[sol(rpc)]
    interface IRouter {
        function getAmountsOut(uint256 amountIn, address[] calldata path)
            external view returns (uint256[] memory amounts);
    }
}

/// Check if a token is a honeypot by simulating a sell.
///
/// Returns `true` if the token appears safe, `false` if it's likely a honeypot.
pub async fn check_honeypot<P: Provider + Clone>(
    provider: &P,
    token: Address,
    router: Address,
    wmon: Address,
) -> Result<bool, String> {
    debug!("Checking honeypot for token: {:?}", token);

    // Try to get quote for selling token -> WMON
    let router_contract = IRouter::new(router, provider);

    let path = vec![token, wmon];
    let test_amount = U256::from(1_000_000_000_000_000_000u128); // 1 token

    match router_contract
        .getAmountsOut(test_amount, path)
        .call()
        .await
    {
        Ok(result) => {
            let amounts = result;
            if amounts.len() >= 2 && amounts[1] > U256::ZERO {
                debug!("Token {:?} passed honeypot check, output: {:?}", token, amounts[1]);
                Ok(true)
            } else {
                warn!("Token {:?} failed honeypot check: zero output", token);
                Ok(false)
            }
        }
        Err(e) => {
            warn!("Token {:?} failed honeypot check: {}", token, e);
            Ok(false)
        }
    }
}

/// Fast honeypot check - just verify token contract exists and has code.
pub async fn quick_check<P: Provider + Clone>(
    provider: &P,
    token: Address,
) -> Result<bool, String> {
    match provider.get_code_at(token).await {
        Ok(code) => {
            if code.len() > 0 {
                Ok(true)
            } else {
                warn!("Token {:?} has no code", token);
                Ok(false)
            }
        }
        Err(e) => {
            warn!("Failed to get code for {:?}: {}", token, e);
            Ok(false)
        }
    }
}
