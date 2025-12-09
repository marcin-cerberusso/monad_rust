// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Event listeners for detecting new tokens.

pub mod nadfun;

pub use nadfun::{spawn_listener, NewTokenEvent};
