//! The message list: the messages of the folder or search shown, the one
//! selected, the date groups folded away and the messages marked.

use std::collections::BTreeSet;

use jiff::Zoned;

use crate::model::{Message, MessageId, sort_messages};
use crate::tui::groups::{self, DateGroup, MessageRow};

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

impl MessageList {
    /// The selected message; none while a date header is selected.
    pub fn selected(&self) -> Option<&Message> {
        if self.header.is_some() {
            return None;
        }
        self.messages.get(self.selected)
    }

    /// The rows as they are at `now`: date headers, and the messages of the
    /// groups not folded away.
    pub fn rows(&self, now: &Zoned) -> Vec<MessageRow> {
        groups::message_rows(&self.messages, &self.folded, now)
    }

    /// The row of `rows` the cursor is on: the selected header, or the
    /// selected message, or the header of the group it is folded in.
    pub fn selected_row(&self, rows: &[MessageRow], now: &Zoned) -> Option<usize> {
        let group = self.header.or_else(|| self.group_at(self.selected, now))?;
        rows.iter().position(|row| match *row {
            MessageRow::Message(index) => self.header.is_none() && index == self.selected,
            MessageRow::Header {
                group: header,
                collapsed,
                ..
            } => header == group && (self.header.is_some() || collapsed),
        })
    }

    /// The date group of the message at `index`, at `now`.
    pub fn group_at(&self, index: usize, now: &Zoned) -> Option<DateGroup> {
        groups::group_at(&self.messages, index, now)
    }

    /// Puts the cursor on `row`: a header, or a message.
    pub fn select_row(&mut self, row: MessageRow, now: &Zoned) {
        match row {
            MessageRow::Header { group, .. } => {
                self.header = Some(group);
                if let Some(first) =
                    (0..self.messages.len()).find(|&i| self.group_at(i, now) == Some(group))
                {
                    self.selected = first;
                }
            }
            MessageRow::Message(index) => {
                self.header = None;
                self.selected = index;
            }
        }
    }

    /// Selects the selected message's row, or its group's header when the
    /// group is folded away.
    pub fn settle(&mut self, now: &Zoned) {
        self.header = self
            .group_at(self.selected, now)
            .filter(|group| self.folded.contains(group));
    }

    /// Lists a newly loaded folder's `messages`, selecting the first and
    /// marking none.
    pub fn replace(&mut self, messages: Vec<Message>, now: &Zoned) {
        self.marked.clear();
        self.messages = messages;
        self.selected = 0;
        self.settle(now);
    }

    /// Lists `messages` after a sync, keeping the selected header while its
    /// group has mail, or else the selected message while it is there, and
    /// the marks of messages still listed. Returns whether another message
    /// is selected instead: the first.
    pub fn refresh(&mut self, messages: Vec<Message>, now: &Zoned) -> bool {
        let selected = self.selected().map(|m| m.id.clone());
        self.marked
            .retain(|id| messages.iter().any(|m| &m.id == id));
        if let Some(group) = self.header
            && let Some(index) =
                (0..messages.len()).find(|&i| groups::group_at(&messages, i, now) == Some(group))
        {
            self.messages = messages;
            self.selected = index;
            return false;
        }
        match selected.and_then(|id| messages.iter().position(|m| m.id == id)) {
            Some(index) => {
                self.messages = messages;
                self.selected = index;
                self.settle(now);
                false
            }
            None => {
                let marked = std::mem::take(&mut self.marked);
                self.replace(messages, now);
                self.marked = marked;
                true
            }
        }
    }

    /// Drops messages that left the list. The selection stays on the same
    /// row, so the message after a dropped one is selected next. Returns
    /// whether another message is selected.
    pub fn drop(&mut self, ids: &[MessageId], now: &Zoned) -> bool {
        let selected = self.selected().map(|m| m.id.clone());
        let gone = |m: &Message| ids.contains(&m.id);
        let before = self.messages.iter().take(self.selected);
        let index = before.filter(|m| !gone(m)).count();
        self.messages.retain(|m| !gone(m));
        self.selected = index.min(self.messages.len().saturating_sub(1));
        self.settle(now);
        for id in ids {
            self.marked.remove(id);
        }
        self.selected().map(|m| &m.id) != selected.as_ref()
    }

    /// Pins or unpins the messages of `ids`, moving them in the list while
    /// the cursor stays on its message.
    pub fn set_pinned(&mut self, ids: &[MessageId], pinned: bool, now: &Zoned) {
        let selected = self.selected().map(|m| m.id.clone());
        for m in self.messages.iter_mut().filter(|m| ids.contains(&m.id)) {
            m.pinned = pinned;
        }
        sort_messages(&mut self.messages);
        if let Some(index) = selected.and_then(|id| self.messages.iter().position(|m| m.id == id)) {
            self.selected = index;
        }
        self.settle(now);
    }

    /// Unfolds the selected header's group. `None` with no header selected;
    /// otherwise whether it was folded.
    pub fn unfold(&mut self) -> Option<bool> {
        let group = self.header?;
        Some(self.folded.remove(&group))
    }

    /// Folds the group of the selected message or header, leaving the
    /// cursor on its header. Returns whether a message was selected before.
    pub fn fold(&mut self, now: &Zoned) -> Option<bool> {
        let group = self.header.or_else(|| self.group_at(self.selected, now))?;
        self.folded.insert(group);
        let moved = self.header.replace(group).is_none();
        self.select_row(
            MessageRow::Header {
                group,
                count: 0,
                collapsed: true,
            },
            now,
        );
        Some(moved)
    }

    /// Folds or unfolds the selected header's group.
    pub fn toggle_fold(&mut self) {
        if let Some(group) = self.header
            && !self.folded.remove(&group)
        {
            self.folded.insert(group);
        }
    }

    /// The marked messages in list order, or else the selected one.
    pub fn to_act_on(&self) -> Vec<MessageId> {
        if self.marked.is_empty() {
            return self.selected().map(|m| m.id.clone()).into_iter().collect();
        }
        let marked = self.messages.iter().filter(|m| self.marked.contains(&m.id));
        marked.map(|m| m.id.clone()).collect()
    }

    /// Marks the selected message, or unmarks it.
    pub fn toggle_mark(&mut self) {
        if let Some(id) = self.selected().map(|m| m.id.clone())
            && !self.marked.remove(&id)
        {
            self.marked.insert(id);
        }
    }

    pub fn mark_all(&mut self) {
        self.marked = self.messages.iter().map(|m| m.id.clone()).collect();
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::*;

    fn now() -> Zoned {
        "2026-11-06T12:00:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .to_zoned(jiff::tz::TimeZone::UTC)
    }

    /// Mail from today, then mail from long ago.
    fn list(today: &[&str], older: &[&str]) -> MessageList {
        let today = today.iter().map(|id| Message {
            received_at: now().timestamp(),
            ..crate::core::fake::message(id)
        });
        let older = older.iter().map(|id| crate::core::fake::message(id));
        let mut list = MessageList::default();
        list.replace(today.chain(older).collect(), &now());
        list
    }

    fn id(id: &str) -> MessageId {
        MessageId(id.to_owned())
    }

    fn selected(list: &MessageList) -> Option<&str> {
        list.selected().map(|m| m.id.0.as_str())
    }

    #[test]
    fn folding_leaves_the_cursor_on_the_header_and_unfolding_keeps_it() {
        let mut list = list(&["t1", "t2"], &["o1"]);
        list.selected = 1;

        assert_eq!(list.fold(&now()), Some(true));
        assert_eq!(selected(&list), None);
        assert_eq!(list.header, Some(DateGroup::Today));
        assert_eq!(list.rows(&now()).len(), 3);

        assert_eq!(list.unfold(), Some(true));
        assert_eq!(list.unfold(), Some(false));
        assert_eq!(list.header, Some(DateGroup::Today));
    }

    #[test]
    fn dropping_the_selected_message_selects_the_next() {
        let mut list = list(&["t1", "t2", "t3"], &[]);
        list.selected = 1;
        list.marked.insert(id("t2"));

        assert!(list.drop(&[id("t2")], &now()));

        assert_eq!(selected(&list), Some("t3"));
        assert!(list.marked.is_empty());
        assert!(!list.drop(&[id("t1")], &now()));
        assert_eq!(selected(&list), Some("t3"));
    }

    #[test]
    fn a_refresh_keeps_the_selected_message_and_marks_still_listed() {
        let mut list = list(&["t1", "t2"], &[]);
        list.selected = 1;
        list.mark_all();
        let messages = list.messages[1..].to_vec();

        assert!(!list.refresh(messages, &now()));

        assert_eq!(selected(&list), Some("t2"));
        assert_eq!(list.to_act_on(), [id("t2")]);
    }

    #[test]
    fn pinning_moves_the_message_and_the_cursor_with_it() {
        let mut list = list(&["t1", "t2"], &[]);
        list.selected = 1;

        list.set_pinned(&[id("t2")], true, &now());

        assert_eq!(list.messages[0].id, id("t2"));
        assert_eq!(selected(&list), Some("t2"));
    }
}
