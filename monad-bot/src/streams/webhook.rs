// Copyright (C) 2025 Category Labs, Inc.
#![allow(unused)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! QuickNode Streams webhook server for real-time blockchain data.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// QuickNode Stream event for ERC20 transfers.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamEvent {
    pub data: Vec<StreamData>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamData {
    pub block: Option<BlockInfo>,
    pub transactions: Option<Vec<TransactionInfo>>,
    pub logs: Option<Vec<LogInfo>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockInfo {
    pub number: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionInfo {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogInfo {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    #[serde(rename = "transactionHash")]
    pub transaction_hash: String,
}

/// Whale transfer detected.
#[derive(Debug, Clone)]
pub struct WhaleTransfer {
    pub from: String,
    pub to: String,
    pub token: String,
    pub amount_wei: String,
    pub tx_hash: String,
}

/// Webhook server state.
pub struct WebhookState {
    pub security_token: String,
    pub whale_tx: mpsc::Sender<WhaleTransfer>,
    pub min_whale_amount_wei: u128,
}

/// Start the webhook server.
pub async fn start_webhook_server(
    port: u16,
    security_token: String,
    whale_tx: mpsc::Sender<WhaleTransfer>,
    min_whale_amount_wei: u128,
) -> Result<(), String> {
    let state = Arc::new(WebhookState {
        security_token,
        whale_tx,
        min_whale_amount_wei,
    });

    let app = Router::new()
        .route("/webhook/quicknode", post(handle_webhook))
        .route("/health", axum::routing::get(health_check))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("🌐 Starting webhook server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind: {}", e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

async fn handle_webhook(
    State(state): State<Arc<WebhookState>>,
    headers: HeaderMap,
    Json(payload): Json<StreamEvent>,
) -> StatusCode {
    // Verify security token
    let auth_header = headers
        .get("x-qn-security")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth_header != state.security_token {
        warn!("Invalid security token");
        return StatusCode::UNAUTHORIZED;
    }

    debug!("Received stream event with {} data items", payload.data.len());

    // Process each data item
    for data in payload.data {
        // Process logs for ERC20 transfers
        if let Some(logs) = data.logs {
            for log in logs {
                process_log(&state, &log).await;
            }
        }

        // Process transactions for large MON transfers
        if let Some(txs) = data.transactions {
            for tx in txs {
                process_transaction(&state, &tx).await;
            }
        }
    }

    StatusCode::OK
}

async fn process_log(state: &WebhookState, log: &LogInfo) {
    // ERC20 Transfer event: Transfer(address from, address to, uint256 value)
    // Topic[0] = 0xddf252ad... (Transfer signature)
    const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

    if log.topics.len() >= 3 && log.topics[0] == TRANSFER_TOPIC {
        let from = &log.topics[1];
        let to = &log.topics[2];
        let amount_hex = &log.data;

        // Parse amount from hex
        let amount_str = amount_hex.trim_start_matches("0x");
        if let Ok(amount) = u128::from_str_radix(amount_str, 16) {
            if amount >= state.min_whale_amount_wei {
                info!(
                    "🐋 WHALE TRANSFER: {} -> {} ({} wei) token {}",
                    from, to, amount, log.address
                );

                let whale = WhaleTransfer {
                    from: from.clone(),
                    to: to.clone(),
                    token: log.address.clone(),
                    amount_wei: amount.to_string(),
                    tx_hash: log.transaction_hash.clone(),
                };

                let _ = state.whale_tx.send(whale).await;
            }
        }
    }
}

async fn process_transaction(state: &WebhookState, tx: &TransactionInfo) {
    // Check for large MON transfers (value > threshold)
    let value_hex = tx.value.trim_start_matches("0x");
    if let Ok(value) = u128::from_str_radix(value_hex, 16) {
        if value >= state.min_whale_amount_wei {
            info!(
                "🐋 WHALE MON TRANSFER: {} -> {:?} ({} wei)",
                tx.from, tx.to, value
            );
        }
    }
}
