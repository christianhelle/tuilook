use jiff::Timestamp;

use crate::model::FolderId;

/// Graph message id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub folder_id: FolderId,
    pub conversation_id: String,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    pub received_at: Timestamp,
    pub is_read: bool,
    pub flagged: bool,
    pub importance: Importance,
    pub has_attachments: bool,
    pub preview: String,
}

/// The fields of a message that changed on the server; `None` leaves a
/// field as it is. Graph reports a change this way, without the message's
/// subject, sender or date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageChange {
    pub id: MessageId,
    pub conversation_id: Option<String>,
    pub subject: Option<String>,
    /// Name and address.
    pub from: Option<(String, String)>,
    pub is_read: Option<bool>,
    pub flagged: Option<bool>,
    pub importance: Option<Importance>,
    pub has_attachments: Option<bool>,
    pub preview: Option<String>,
}

impl MessageChange {
    /// A change to `id` that changes nothing yet.
    pub fn new(id: MessageId) -> Self {
        Self {
            id,
            conversation_id: None,
            subject: None,
            from: None,
            is_read: None,
            flagged: None,
            importance: None,
            has_attachments: None,
            preview: None,
        }
    }
}

/// Orders messages the way the message list shows them: newest first.
pub fn sort_messages(messages: &mut [Message]) {
    messages.sort_by_key(|m| std::cmp::Reverse(m.received_at));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, received_at: &str) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("inbox".to_owned()),
            conversation_id: String::new(),
            subject: String::new(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: received_at.parse().unwrap(),
            is_read: false,
            flagged: false,
            importance: Importance::Normal,
            has_attachments: false,
            preview: String::new(),
        }
    }

    #[test]
    fn newest_messages_sort_first() {
        let mut messages = vec![
            message("old", "2026-01-01T09:00:00Z"),
            message("new", "2026-03-01T09:00:00Z"),
            message("mid", "2026-02-01T09:00:00Z"),
        ];

        sort_messages(&mut messages);

        let ids: Vec<_> = messages.iter().map(|m| m.id.0.as_str()).collect();
        assert_eq!(ids, ["new", "mid", "old"]);
    }
}
