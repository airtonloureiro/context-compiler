use crate::core::context_item::{ContextItem, ContextItemType, Sensitivity};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub is_pinned: Option<bool>,
}

pub struct ChatAdapter;

impl ChatAdapter {
    /// Ingests conversation history and prepares context items.
    /// Pinned messages (like user profile, confirmed facts) will get critical metadata.
    pub fn ingest(history: Vec<ChatMessage>, current_question: &str) -> Vec<ContextItem> {
        let mut items = Vec::new();

        // 1. Current Question (Highest priority)
        items.push(ContextItem {
            id: "current_question".to_string(),
            item_type: ContextItemType::Message,
            role: Some("user".to_string()),
            content: current_question.to_string(),
            source: None,
            metadata: Some(serde_json::json!({"critical": true})),
            sensitivity: Sensitivity::Public,
        });

        // 2. History Messages
        for (i, msg) in history.into_iter().enumerate() {
            let is_critical = msg.is_pinned.unwrap_or(false);
            items.push(ContextItem {
                id: format!("history_msg_{}", i),
                item_type: ContextItemType::Message,
                role: Some(msg.role),
                content: msg.content,
                source: Some(serde_json::json!({"kind": "chat_history"})),
                metadata: if is_critical {
                    Some(serde_json::json!({"critical": true}))
                } else {
                    None
                },
                sensitivity: Sensitivity::Public,
            });
        }

        items
    }
}
