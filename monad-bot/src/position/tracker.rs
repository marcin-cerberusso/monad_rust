// Copyright (C) 2025 Category Labs, Inc.
#![allow(dead_code)]
// SPDX-License-Identifier: GPL-3.0-or-later

//! Position tracking for open trades.

use alloy::primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, error, info};

const POSITIONS_FILE: &str = "positions.json";

/// A single position (token holding).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub token: Address,
    pub name: String,
    pub symbol: String,
    pub amount: U256,
    pub buy_price_mon: f64,
    pub buy_time: u64,
    pub highest_price: f64,
    pub tx_hash: String,
}

/// Manages all open positions.
#[derive(Debug, Default)]
pub struct PositionTracker {
    positions: HashMap<Address, Position>,
}

impl PositionTracker {
    /// Create a new position tracker.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
        }
    }

    /// Load positions from file.
    pub fn load() -> Self {
        let path = Path::new(POSITIONS_FILE);
        if !path.exists() {
            info!("No positions file found, starting fresh");
            return Self::new();
        }

        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<HashMap<Address, Position>>(&content) {
                Ok(positions) => {
                    info!("Loaded {} positions from file", positions.len());
                    Self { positions }
                }
                Err(e) => {
                    error!("Failed to parse positions file: {}", e);
                    Self::new()
                }
            },
            Err(e) => {
                error!("Failed to read positions file: {}", e);
                Self::new()
            }
        }
    }

    /// Save positions to file.
    pub fn save(&self) -> Result<(), String> {
        let content = serde_json::to_string_pretty(&self.positions)
            .map_err(|e| format!("Failed to serialize positions: {}", e))?;

        fs::write(POSITIONS_FILE, content)
            .map_err(|e| format!("Failed to write positions file: {}", e))?;

        debug!("Saved {} positions to file", self.positions.len());
        Ok(())
    }

    /// Add a new position.
    pub fn add(&mut self, position: Position) {
        info!(
            "Adding position: {} ({}) - {} tokens",
            position.name, position.symbol, position.amount
        );
        self.positions.insert(position.token, position);
        let _ = self.save();
    }

    /// Remove a position.
    pub fn remove(&mut self, token: &Address) -> Option<Position> {
        let position = self.positions.remove(token);
        if position.is_some() {
            let _ = self.save();
        }
        position
    }

    /// Get a position by token address.
    pub fn get(&self, token: &Address) -> Option<&Position> {
        self.positions.get(token)
    }

    /// Get mutable position by token address.
    pub fn get_mut(&mut self, token: &Address) -> Option<&mut Position> {
        self.positions.get_mut(token)
    }

    /// Update highest price for a position.
    pub fn update_highest_price(&mut self, token: &Address, price: f64) {
        if let Some(pos) = self.positions.get_mut(token) {
            if price > pos.highest_price {
                pos.highest_price = price;
                debug!(
                    "Updated highest price for {} ({}): {}",
                    pos.name, pos.symbol, price
                );
                let _ = self.save();
            }
        }
    }

    /// Get all positions.
    pub fn all(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    /// Get number of positions.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}
