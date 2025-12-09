// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider setup and configuration for Monad RPC.

use alloy::{
    network::EthereumWallet,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};

/// Configuration for RPC connection.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub rpc_url: String,
    pub private_key: String,
    pub chain_id: u64,
}

impl RpcConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        let rpc_url = std::env::var("MONAD_RPC_URL")
            .map_err(|_| "MONAD_RPC_URL not set")?;
        let private_key = std::env::var("PRIVATE_KEY")
            .map_err(|_| "PRIVATE_KEY not set")?;
        let chain_id = std::env::var("CHAIN_ID")
            .unwrap_or_else(|_| "10143".to_string()) // Monad testnet default
            .parse()
            .map_err(|_| "Invalid CHAIN_ID")?;

        Ok(Self {
            rpc_url,
            private_key,
            chain_id,
        })
    }
}

/// Create a provider with signer from config.
pub fn create_provider(
    config: &RpcConfig,
) -> Result<(impl Provider + Clone, EthereumWallet), String> {
    let signer: PrivateKeySigner = config
        .private_key
        .parse()
        .map_err(|e| format!("Invalid private key: {e}"))?;

    let wallet = EthereumWallet::from(signer);

    let url: Url = config
        .rpc_url
        .parse()
        .map_err(|e| format!("Invalid RPC URL: {e}"))?;

    let provider = ProviderBuilder::new()
        .wallet(wallet.clone())
        .connect_http(url);

    Ok((provider, wallet))
}
