// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! nad.fun SDK-based event listener using official CurveStream.

use alloy::primitives::Address;
use futures_util::{pin_mut, StreamExt};
use nadfun_sdk::stream::CurveStream;
use nadfun_sdk::types::{BondingCurveEvent, EventType};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Token event from nad.fun bonding curve.
#[derive(Debug, Clone)]
pub struct TokenEvent {
    pub event_type: EventType,
    pub token: Address,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub creator: Option<Address>,
    pub trader: Option<Address>,
    pub amount_in: Option<u128>,
    pub amount_out: Option<u128>,
    pub initial_liquidity: Option<u128>, // Added for compatibility
    pub block_number: u64,
}

/// Spawn the CurveStream listener as a background task.
pub fn spawn_curve_stream(
    ws_url: String,
    tx: mpsc::Sender<TokenEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🔌 Connecting to nad.fun CurveStream...");

        loop {
            match CurveStream::new(ws_url.clone()).await {
                Ok(curve_stream) => {
                    info!("✅ Connected to nad.fun CurveStream");

                    // Subscribe to Create, Buy, Sell events
                    let curve_stream = curve_stream
                        .subscribe_events(vec![EventType::Create, EventType::Buy, EventType::Sell]);

                    match curve_stream.subscribe().await {
                        Ok(stream) => {
                            pin_mut!(stream);

                            while let Some(event_result) = stream.next().await {
                                match event_result {
                                    Ok(event) => {
                                        let token_event = match &event {
                                            BondingCurveEvent::Create(e) => {
                                                info!(
                                                    "🆕 NEW TOKEN: {} ({}) at {:?}",
                                                    e.name, e.symbol, e.token
                                                );
                                                TokenEvent {
                                                    event_type: EventType::Create,
                                                    token: e.token,
                                                    name: Some(e.name.clone()),
                                                    symbol: Some(e.symbol.clone()),
                                                    creator: Some(e.creator),
                                                    trader: None,
                                                    amount_in: None,
                                                    amount_out: None,
                                                    initial_liquidity: None, // Will be fetched on-chain
                                                    block_number: e.block_number,
                                                }
                                            }
                                            BondingCurveEvent::Buy(e) => {
                                                debug!(
                                                    "📈 BUY: {:?} | In: {} | Out: {}",
                                                    e.token,
                                                    e.amount_in,
                                                    e.amount_out
                                                );
                                                TokenEvent {
                                                    event_type: EventType::Buy,
                                                    token: e.token,
                                                    name: None,
                                                    symbol: None,
                                                    creator: None,
                                                    trader: Some(e.sender),
                                                    amount_in: Some(e.amount_in.to::<u128>()),
                                                    amount_out: Some(e.amount_out.to::<u128>()),
                                                    initial_liquidity: None,
                                                    block_number: e.block_number,
                                                }
                                            }
                                            BondingCurveEvent::Sell(e) => {
                                                debug!(
                                                    "📉 SELL: {:?} | In: {} | Out: {}",
                                                    e.token,
                                                    e.amount_in,
                                                    e.amount_out
                                                );
                                                TokenEvent {
                                                    event_type: EventType::Sell,
                                                    token: e.token,
                                                    name: None,
                                                    symbol: None,
                                                    creator: None,
                                                    trader: Some(e.sender),
                                                    amount_in: Some(e.amount_in.to::<u128>()),
                                                    amount_out: Some(e.amount_out.to::<u128>()),
                                                    initial_liquidity: None,
                                                    block_number: e.block_number,
                                                }
                                            }
                                            BondingCurveEvent::Graduate(e) => {
                                                info!(
                                                    "🎓 GRADUATED: {:?} -> Pool: {:?}",
                                                    e.token, e.pool
                                                );
                                                TokenEvent {
                                                    event_type: EventType::Graduate,
                                                    token: e.token,
                                                    name: None,
                                                    symbol: None,
                                                    creator: None,
                                                    trader: None,
                                                    amount_in: None,
                                                    amount_out: None,
                                                    initial_liquidity: None,
                                                    block_number: e.block_number,
                                                }
                                            }
                                            _ => continue, // Skip Sync/Lock
                                        };

                                        // Send to channel
                                        if let Err(e) = tx.send(token_event).await {
                                            warn!("Failed to send token event: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("CurveStream error: {}", e);
                                    }
                                }
                            }

                            warn!("CurveStream ended, reconnecting...");
                        }
                        Err(e) => {
                            error!("Failed to subscribe to CurveStream: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to CurveStream: {}", e);
                }
            }

            // Reconnect delay
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    })
}
