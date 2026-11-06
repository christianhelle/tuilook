//! The message list: the messages of the folder or search shown, the one
//! selected, the date groups folded away and the messages marked.

use std::collections::BTreeSet;

use crate::model::{Message, MessageId};
use crate::tui::groups::DateGroup;

#[derive(Debug, Default)]
pub struct MessageList {
    pub messages: Vec<Message>,
    /// The index of the selected message.
    pub selected: usize,
    /// The date group whose header is selected, instead of a message. The
    /// selected message is then the group's first.
    pub header: Option<DateGroup>,
    /// Date groups whose messages are folded away, in every folder.
    pub folded: BTreeSet<DateGroup>,
    /// Messages picked for acting on together, such as deleting.
    pub marked: BTreeSet<MessageId>,
}
