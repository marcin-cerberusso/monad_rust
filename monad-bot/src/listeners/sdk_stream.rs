// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! nad.fun SDK-based event listener using official CurveStream.

use alloy::primitives::{Address, B256, U256};
use futures_util::{pin_mut, StreamExt};
use nadfun_sdk::stream::CurveStream;
use nadfun_sdk::types::{BondingCurveEvent, EventType};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Event emitted when a new token is created.
/// Compatible with the legacy listener interface.
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

/// Event emitted when a smart wallet buys - triggers copy trade.
#[derive(Debug, Clone)]
pub struct CopyTradeEvent {
    pub token: Address,
    pub smart_wallet: Address,
    pub amount_in: U256,
    pub amount_out: U256,
    pub is_buy: bool, // true = buy, false = sell
    pub is_scout_only: bool, // true = observe only, do not copy
}

/// Spawn the CurveStream listener as a background task.
/// This replaces the legacy `nadfun::spawn_listener`.
/// 
/// # Arguments
/// * `ws_url` - WebSocket URL for nad.fun CurveStream
/// * `tx` - Channel to send new token events
/// * `copy_tx` - Channel to send copy trade events when smart wallets trade
/// * `smart_wallets` - List of wallet addresses to track as "smart money"
pub fn spawn_listener(
    ws_url: String,
    tx: mpsc::Sender<NewTokenEvent>,
    copy_tx: mpsc::Sender<CopyTradeEvent>,
    smart_wallets: Vec<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🔌 Connecting to nad.fun CurveStream...");
        if !smart_wallets.is_empty() {
            info!("👀 Tracking {} smart wallets for copy trading", smart_wallets.len());
        }



        loop {
            match CurveStream::new(ws_url.clone()).await {
                Ok(curve_stream) => {
                    info!("✅ Connected to nad.fun CurveStream");

                    // Subscribe to Create events for new tokens
                    // We also subscribe to Buy/Sell for logging/debugging, but main loop only cares about Create for now
                    let curve_stream = curve_stream
                        .subscribe_events(vec![EventType::Create, EventType::Buy, EventType::Sell]);

                    match curve_stream.subscribe().await {
                        Ok(stream) => {
                            pin_mut!(stream);

                            while let Some(event_result) = stream.next().await {
                                match event_result {
                                    Ok(event) => {
                                        match event {
                                            BondingCurveEvent::Create(e) => {
                                                info!(
                                                    "🆕 NEW TOKEN: {} ({}) at {:?}",
                                                    e.name, e.symbol, e.token
                                                );
                                                
                                                let event = NewTokenEvent {
                                                    token_address: e.token,
                                                    name: e.name,
                                                    symbol: e.symbol,
                                                    creator: Some(e.creator),
                                                    bonding_curve: Some(e.pool),
                                                    initial_liquidity: None, // SDK create event might not have this, strategy handles None or fetching
                                                    timestamp: Some(chrono::Utc::now().timestamp() as u64),
                                                    tx_hash: None, // Stream might not provide tx hash directly in event struct yet
                                                };

                                                // Send to channel
                                                if let Err(e) = tx.send(event).await {
                                                    warn!("Failed to send token event: {}", e);
                                                }
                                            }
                                            BondingCurveEvent::Buy(e) => {
                                                let sender = e.sender;
                                                let sender_lower = format!("{:?}", sender).to_lowercase();
                                                
                                                let is_target = smart_wallets.iter().any(|w| sender_lower.contains(w));
                                                
                                                // Calculate value roughly (amount_in is MON for Buy)
                                                // Note: U256 to f64 helper needed or simple conversion
                                                let val_str = e.amount_in.to_string();
                                                let val_f64: f64 = val_str.parse().unwrap_or(0.0) / 1e18;

                                                // Scout Filter: Ignore small unknown trades (< 5.0 MON)
                                                if !is_target && val_f64 < 5.0 {
                                                    continue;
                                                }

                                                if is_target {
                                                    info!("🚨 SMART MONEY BUY: {:?} | Amount: {} | Sender: {:?}", e.token, e.amount_in, sender);
                                                }

                                                // Send event
                                                let copy_event = CopyTradeEvent {
                                                    token: e.token,
                                                    smart_wallet: sender,
                                                    amount_in: e.amount_in,
                                                    amount_out: e.amount_out,
                                                    is_buy: true,
                                                    is_scout_only: !is_target,
                                                };
                                                if let Err(err) = copy_tx.send(copy_event).await {
                                                    warn!("Failed to send event: {}", err);
                                                }
                                                debug!("📈 BUY: {:?} | In: {} | Out: {}", e.token, e.amount_in, e.amount_out);
                                            }
                                            BondingCurveEvent::Sell(e) => {
                                                let sender = e.sender;
                                                let sender_lower = format!("{:?}", sender).to_lowercase();


                                                let is_target = smart_wallets.iter().any(|w| sender_lower.contains(w));

                                                // Calculate value roughly (amount_out is MON for Sell)
                                                let val_str = e.amount_out.to_string();
                                                let val_f64: f64 = val_str.parse().unwrap_or(0.0) / 1e18;

                                                // Scout Filter: Ignore small unrecgonized sells
                                                if !is_target && val_f64 < 5.0 {
                                                    continue;
                                                }

                                                if is_target {
                                                    info!("🚨 SMART MONEY SELL: {:?} | Amount: {} | Sender: {:?}", e.token, e.amount_in, sender);
                                                }

                                                // Send copy trade event for sells too!
                                                let copy_event = CopyTradeEvent {
                                                    token: e.token,
                                                    smart_wallet: sender,
                                                    amount_in: e.amount_in,
                                                    amount_out: e.amount_out,
                                                    is_buy: false,
                                                    is_scout_only: !is_target,
                                                };
                                                if let Err(err) = copy_tx.send(copy_event).await {
                                                    warn!("Failed to send event: {}", err);
                                                }

                                                debug!("📉 SELL: {:?} | In: {} | Out: {}", e.token, e.amount_in, e.amount_out);
                                            }
                                            BondingCurveEvent::Graduate(e) => {
                                                info!("🎓 GRADUATED: {:?} -> Pool: {:?}", e.token, e.pool);
                                            }
                                            _ => {}
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
                    error!("Failed to connect to CurveStream ({}). Retrying...", e);
                }
            }

            // Reconnect delay
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    })
}
