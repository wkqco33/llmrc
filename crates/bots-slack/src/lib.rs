//! Slack event mapping. Enable the `slack-morphism` feature when integrating
//! with slack-morphism's event types.
use llmrc_bots_core::{BotEvent, BotResponse, ConversationId, ConversationKey};

#[derive(Debug, Clone, Default)]
pub struct SlackAdapter;

impl SlackAdapter {
    pub fn event(
        channel_id: impl Into<String>,
        author: Option<String>,
        text: impl Into<String>,
    ) -> BotEvent {
        BotEvent {
            key: ConversationKey::new("slack", ConversationId::new(channel_id.into())),
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
    fn event_creates_slack_bot_event() {
        let event = SlackAdapter::event("C12345", Some("U67890".into()), "hello slack");
        assert_eq!(event.key.platform, "slack");
        assert_eq!(event.key.id.as_str(), "C12345");
        assert_eq!(event.author.as_deref(), Some("U67890"));
        assert_eq!(event.text, "hello slack");
    }

    #[test]
    fn response_text_extracts_content() {
        let response = BotResponse {
            key: ConversationKey::new("slack", "C12345"),
            text: "pong".into(),
            reply_to: None,
        };
        assert_eq!(SlackAdapter::response_text(&response), "pong");
    }
}
