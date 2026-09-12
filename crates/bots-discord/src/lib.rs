//! Discord event mapping. Enable the `serenity` feature in an application
//! that wants to pair this mapper with Serenity's gateway types.
use llmrc_bots_core::{BotEvent, BotResponse, ConversationId, ConversationKey};

#[derive(Debug, Clone, Default)]
pub struct DiscordAdapter;

impl DiscordAdapter {
    pub fn event(
        channel_id: impl Into<String>,
        author: Option<String>,
        text: impl Into<String>,
    ) -> BotEvent {
        BotEvent {
            key: ConversationKey::new("discord", ConversationId::new(channel_id.into())),
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
    fn event_creates_discord_bot_event() {
        let event = DiscordAdapter::event("channel-123", Some("alice".into()), "hello discord");
        assert_eq!(event.key.platform, "discord");
        assert_eq!(event.key.id.as_str(), "channel-123");
        assert_eq!(event.author.as_deref(), Some("alice"));
        assert_eq!(event.text, "hello discord");
    }

    #[test]
    fn response_text_extracts_content() {
        let response = BotResponse {
            key: ConversationKey::new("discord", "channel-123"),
            text: "pong".into(),
            reply_to: Some("msg-1".into()),
        };
        assert_eq!(DiscordAdapter::response_text(&response), "pong");
    }
}
