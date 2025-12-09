// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Position management module.

pub mod tracker;
pub mod trailing_sl;

pub use tracker::{Position, PositionTracker};
pub use trailing_sl::{spawn_monitor, SellDecision, TrailingStopLossConfig};
