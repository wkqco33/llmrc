//! Telegram event mapping. Enable the `teloxide` feature when integrating
//! with teloxide's update loop.
use llmrc_bots_core::{BotEvent, BotResponse, ConversationId, ConversationKey};

#[derive(Debug, Clone, Default)]
pub struct TelegramAdapter;

impl TelegramAdapter {
    pub fn event(
        chat_id: impl Into<String>,
        author: Option<String>,
        text: impl Into<String>,
    ) -> BotEvent {
        BotEvent {
            key: ConversationKey::new("telegram", ConversationId::new(chat_id.into())),
            message_id: None,
            author,
            text: text.into(),
        }
    }

    pub fn response_text(response: &BotResponse) -> &str {
        &response.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_creates_telegram_bot_event() {
        let event = TelegramAdapter::event("987654", Some("tg_user".into()), "hello telegram");
        assert_eq!(event.key.platform, "telegram");
        assert_eq!(event.key.id.as_str(), "987654");
        assert_eq!(event.author.as_deref(), Some("tg_user"));
        assert_eq!(event.text, "hello telegram");
    }

    #[test]
    fn response_text_extracts_content() {
        let response = BotResponse {
            key: ConversationKey::new("telegram", "987654"),
            text: "pong".into(),
            reply_to: Some("msg-42".into()),
        };
        assert_eq!(TelegramAdapter::response_text(&response), "pong");
    }
}
