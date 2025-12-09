// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mempool Monitor for Front-Running Smart Wallets.

use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::{json, Value};
use tracing::{info, warn, error};
use std::sync::Arc;
use crate::config::Config;
use crate::executor::SdkExecutor;
use alloy::primitives::Address;
use std::str::FromStr;

pub struct MempoolMonitor {
    config: Config,
    sdk: Arc<SdkExecutor>,
}

impl MempoolMonitor {
    pub fn new(config: Config, sdk: Arc<SdkExecutor>) -> Self {
        Self { config, sdk }
    }

    pub async fn start(&self) {
        let ws_url = &self.config.ws_url;
        info!("🔌 Connecting to Mempool stream: {}", ws_url);

        let (ws_stream, _) = match connect_async(ws_url).await {
            Ok(s) => s,
            Err(e) => {
                error!("❌ Failed to connect to Mempool WS: {}", e);
                return;
            }
        };

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to Alchemy pending transactions with filter
        let smart_wallets: Vec<String> = self.config.smart_wallets.clone();
        
        info!("👀 Subscribing to txs from {} smart wallets...", smart_wallets.len());

        let subscribe_msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_subscribe",
            "params": [
                "alchemy_pendingTransactions",
                {
                    "fromAddress": smart_wallets
                }
            ]
        });

        if let Err(e) = write.send(Message::Text(subscribe_msg.to_string())).await {
            error!("❌ Failed to subscribe to mempool: {}", e);
            return;
        }

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Ignore subscription confirmation (result: hex string)
                    if text.contains("alchemy_pendingTransactions") {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            self.handle_message(value).await;
                        }
                    }
                }
                Err(e) => {
                    error!("Mempool WS error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        
        warn!("⚠️ Mempool monitor disconnected");
    }

    async fn handle_message(&self, msg: Value) {
        if let Some(params) = msg.get("params") {
            if let Some(result) = params.get("result") {
                // info!("🔍 Mempool Event: {:?}", result); // Debug all events
                
                let to_addr_str = result.get("to").and_then(|v| v.as_str());
                let input_str = result.get("input").and_then(|v| v.as_str());
                let from_addr = result.get("from").and_then(|v| v.as_str()).unwrap_or("unknown");
                
                if let (Some(to), Some(input)) = (to_addr_str, input_str) {
                    // Check if target is Router
                    let router_addr = format!("{:?}", self.config.router_address).to_lowercase();
                    
                    if to.to_lowercase() == router_addr {
                        // Check for 'buy' selector: 0xf340fa01
                        if input.starts_with("0xf340fa01") {
                            // Extract token address (param 1)
                            // "0x" (2) + selector (8) + padding (24) + address (40)
                            // Offset: 2 + 8 + 24 = 34
                            if input.len() >= 74 {
                                let token_hex = &input[34..74]; 
                                if let Ok(token_address) = Address::from_str(&format!("0x{}", token_hex)) {
                                    info!("🚨 MEMPOOL SNIPE DETECTED! Smart Wallet {} buying {:?}", from_addr, token_address);
                                    
                                    // Calculate front-run gas
                                    let victim_gas_price_hex = result.get("gasPrice").and_then(|v| v.as_str()).unwrap_or("0x0");
                                    let victim_gas_price = u128::from_str_radix(victim_gas_price_hex.trim_start_matches("0x"), 16).unwrap_or(0);
                                    
                                    // Gas War: Victim + 25% or at least +2 gwei
                                    let my_gas_price = if victim_gas_price > 0 {
                                        victim_gas_price + (victim_gas_price / 4)
                                    } else {
                                        50_000_000_000 // Fallback 50 gwei
                                    };
                                    
                                    info!("🔥 Front-running with gas: {} wei (Victim: {})", my_gas_price, victim_gas_price);
                                    
                                    // Execute Buy
                                    let amount = self.config.mon_to_wei(self.config.snipe_amount_mon);
                                    
                                    // Trigger buy in background
                                    let sdk = self.sdk.clone();
                                    let token = token_address;
                                    tokio::spawn(async move {
                                        match sdk.buy_token_with_gas(token, amount, my_gas_price).await {
                                            Ok(_) => info!("✅ Front-run transaction successful!"),
                                            Err(e) => error!("❌ Front-run failed: {}", e),
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
