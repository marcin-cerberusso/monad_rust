// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Blockchain event listener for new token events via QuickNode WebSocket.

use alloy::primitives::{Address, B256, U256};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

/// nad.fun Bonding Curve Router contract address on Monad.
const BONDING_CURVE_ROUTER: &str = "0x4F5A3518F082275edf59026f72B66AC2838c0414";

/// Bonding Curve contract address.
const BONDING_CURVE: &str = "0x52D34d8536350Cd997bCBD0b9E9d722452f341F5";

/// Event emitted when a new token is created.
#[derive(Debug, Clone)]
pub struct NewTokenEvent {
    pub token_address: Address,
    pub name: String,
    pub symbol: String,
    pub creator: Option<Address>,
    pub bonding_curve: Option<Address>,
    pub initial_liquidity: Option<U256>,
    pub timestamp: Option<u64>,
    pub tx_hash: Option<B256>,
}

impl NewTokenEvent {
    /// Create from log data.
    pub fn from_log(token: Address, tx_hash: B256) -> Self {
        Self {
            token_address: token,
            name: String::new(),
            symbol: String::new(),
            creator: None,
            bonding_curve: None,
            initial_liquidity: None,
            timestamp: Some(chrono::Utc::now().timestamp() as u64),
            tx_hash: Some(tx_hash),
        }
    }
}

/// JSON-RPC request for eth_subscribe.
#[derive(Debug, Serialize)]
struct SubscribeRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

/// JSON-RPC response.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<serde_json::Value>,
    method: Option<String>,
    params: Option<SubscriptionParams>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SubscriptionParams {
    subscription: String,
    result: LogResult,
}

#[derive(Debug, Deserialize)]
struct LogResult {
    address: Address,
    topics: Vec<B256>,
    data: String,
    #[serde(rename = "transactionHash")]
    transaction_hash: B256,
    #[serde(rename = "blockNumber")]
    block_number: String,
}

/// Blockchain event listener using QuickNode WebSocket.
pub struct NadFunListener {
    ws_url: String,
    tx: mpsc::Sender<NewTokenEvent>,
}

impl NadFunListener {
    /// Create a new listener.
    pub fn new(ws_url: String, tx: mpsc::Sender<NewTokenEvent>) -> Self {
        Self { ws_url, tx }
    }

    /// Start listening for new token events.
    pub async fn run(&self) {
        loop {
            match self.connect_and_listen().await {
                Ok(_) => {
                    warn!("WebSocket disconnected, reconnecting in 5s...");
                }
                Err(e) => {
                    error!("WebSocket error: {}, reconnecting in 5s...", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    async fn connect_and_listen(&self) -> Result<(), String> {
        info!("Connecting to Monad WebSocket: {}", self.ws_url);

        let (ws_stream, _) = connect_async(&self.ws_url)
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        info!("Connected to Monad WebSocket");

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to logs from Bonding Curve Router
        // This will catch TokenCreated events
        let subscribe = SubscribeRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "eth_subscribe".to_string(),
            params: vec![
                serde_json::json!("logs"),
                serde_json::json!({
                    "address": [BONDING_CURVE_ROUTER, BONDING_CURVE]
                }),
            ],
        };

        let subscribe_msg = serde_json::to_string(&subscribe)
            .map_err(|e| format!("Failed to serialize: {}", e))?;

        write
            .send(Message::Text(subscribe_msg))
            .await
            .map_err(|e| format!("Failed to send subscribe: {}", e))?;

        info!("Subscribed to Bonding Curve logs");

        // Listen for messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    self.handle_message(&text).await;
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    warn!("WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    error!("WebSocket receive error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn handle_message(&self, text: &str) {
        debug!("Received: {}", text);

        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(text) {
            // Check for subscription confirmation
            if let Some(result) = &response.result {
                if result.is_string() {
                    info!("Subscription confirmed: {}", result);
                    return;
                }
            }

            // Check for error
            if let Some(error) = &response.error {
                error!("RPC error: {} - {}", error.code, error.message);
                return;
            }

            // Check for log event
            if let Some(params) = response.params {
                self.handle_log(params.result).await;
            }
        }
    }

    async fn handle_log(&self, log: LogResult) {
        info!("📡 Log from {:?}", log.address);

        // Parse topics to get token address
        // TokenCreated event typically has token address in topics[1]
        if log.topics.len() >= 2 {
            // Extract token address from topic (last 20 bytes of 32-byte topic)
            let topic_bytes = log.topics[1].as_slice();
            let mut addr_bytes: [u8; 20] = [0u8; 20];
            addr_bytes.copy_from_slice(&topic_bytes[12..32]);
            let token_address = Address::from(addr_bytes);

            let event = NewTokenEvent::from_log(token_address, log.transaction_hash);

            info!(
                "🆕 New token detected: {:?} (tx: {:?})",
                event.token_address, event.tx_hash
            );

            if let Err(e) = self.tx.send(event).await {
                error!("Failed to send token event: {}", e);
            }
        } else {
            // Just emit with log address
            let event = NewTokenEvent::from_log(log.address, log.transaction_hash);
            info!(
                "📝 Bonding curve event from {:?}",
                log.address
            );

            if let Err(e) = self.tx.send(event).await {
                error!("Failed to send event: {}", e);
            }
        }
    }
}

/// Start the listener in a background task.
pub fn spawn_listener(ws_url: String, tx: mpsc::Sender<NewTokenEvent>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener = NadFunListener::new(ws_url, tx);
        listener.run().await;
    })
}
