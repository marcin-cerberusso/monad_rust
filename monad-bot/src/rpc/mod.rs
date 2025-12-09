// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! RPC module for interacting with Monad blockchain.

mod executor;
mod provider;

pub use provider::{create_provider, RpcConfig};
