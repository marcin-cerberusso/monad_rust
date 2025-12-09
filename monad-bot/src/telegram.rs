// Copyright (C) 2025 Category Labs, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Telegram notifier module.

use teloxide::prelude::*;
use tracing::{error, info};

#[derive(Clone)]
pub struct TelegramNotifier {
    bot: Option<Bot>,
    chat_id: Option<ChatId>,
}

impl TelegramNotifier {
    pub fn new(token: Option<String>, chat_id: Option<String>) -> Self {
        info!("📱 Initializing Telegram: token={}, chat_id={}", 
              token.as_ref().map(|_| "SET").unwrap_or("NONE"),
              chat_id.as_ref().map(|_| "SET").unwrap_or("NONE"));
        
        let bot = token.map(Bot::new);
        let chat_id = chat_id.map(|id| {
            if let Ok(num) = id.parse::<i64>() {
                ChatId(num)
            } else {
                ChatId(0) // Invalid chat ID, won't send
            }
        });

        Self { bot, chat_id }
    }

    pub async fn send_message(&self, message: &str) {
        if let (Some(bot), Some(chat_id)) = (&self.bot, &self.chat_id) {
            let result = bot.send_message(*chat_id, message).await;
            match result {
                Ok(_) => info!("📤 Sent Telegram message"),
                Err(e) => error!("Failed to send Telegram message: {}", e),
            }
        }
    }
}
