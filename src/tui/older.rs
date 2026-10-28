//! Mail older than sync reaches: fetched a page at a time once the message
//! list is scrolled to its end, or shows nothing, while the folder's counts
//! say there is more.

use crate::model::FolderId;
use jiff::Timestamp;

use crate::tui::app::{AppState, Effect, Status, is_account_row, row_target};

/// Shown while older mail is fetched.
pub const LOADING_OLDER: &str = "Loading older mail…";

impl AppState {
    /// Asks for the next page of older mail when the list shows its last
    /// message, or none, and the folder holds more than the list does.
    pub fn older_wanted(&mut self) -> Option<Effect> {
        if self.loading_older.is_some() || self.results_query().is_some() {
            return None;
        }
        let row = self.folders.get(self.selected_folder)?;
        if is_account_row(row) {
            return None;
        }
        let at_end = self.selected_message + 1 >= self.messages.len();
        let unread_only = self.shows_unread();
        let folder = row_target(&row.id);
        let key = (folder.clone(), unread_only);
        if !at_end || self.no_older.contains(&key) {
            return None;
        }
        let (held, shown) = if unread_only {
            let unread = self.messages.iter().filter(|m| !m.is_read).count();
            (row.unread, unread)
        } else {
            (row.total, self.messages.len())
        };
        if usize::try_from(held).unwrap_or(usize::MAX) <= shown {
            return None;
        }
        let before = self
            .messages
            .last()
            .map_or_else(Timestamp::now, |m| m.received_at);
        self.loading_older = Some(key);
        self.show_busy(LOADING_OLDER);
        Some(Effect::LoadOlder {
            folder,
            before,
            unread_only,
        })
    }

    /// Core fetched a page of older mail; with `more`, the next page can be
    /// asked for once the list is scrolled to its end again.
    pub fn older_loaded(&mut self, folder: FolderId, unread_only: bool, more: bool) {
        let key = (folder, unread_only);
        if self.loading_older.as_ref() == Some(&key) {
            self.loading_older = None;
        }
        if !more {
            self.no_older.insert(key);
        }
        if self.status == Some(Status::Busy(LOADING_OLDER.to_owned())) {
            self.status = None;
            self.status_since = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, Folder, Message, MessageId};

    use super::*;
    use crate::tui::app::{Pane, unread_row};

    fn folder(id: &str, unread: u32, total: u32) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: None,
            name: id.to_owned(),
            well_known: None,
            unread,
            total,
        }
    }

    fn message(id: &str, day: i64, is_read: bool) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("devops".to_owned()),
            conversation_id: String::new(),
            subject: id.to_owned(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: Timestamp::from_second(day * 86_400).unwrap(),
            is_read,
            flagged: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    fn devops(unread: u32, total: u32) -> AppState {
        let mut app = AppState::default();
        app.set_folders(vec![folder("devops", unread, total)]);
        app
    }

    fn older(before: Timestamp, unread_only: bool) -> Option<Effect> {
        Some(Effect::LoadOlder {
            folder: FolderId("devops".to_owned()),
            before,
            unread_only,
        })
    }

    #[test]
    fn a_folder_that_shows_nothing_but_holds_mail_fetches_older_mail() {
        let mut app = devops(0, 3);
        app.set_messages(Vec::new());

        let effect = app.older_wanted();

        assert!(matches!(
            effect,
            Some(Effect::LoadOlder {
                unread_only: false,
                ..
            })
        ));
        assert_eq!(app.status, Some(Status::Busy(LOADING_OLDER.to_owned())));
        assert_eq!(app.older_wanted(), None, "one page at a time");
    }

    #[test]
    fn older_mail_is_fetched_once_the_list_reaches_its_end() {
        let mut app = devops(0, 5);
        app.set_messages(vec![message("m2", 2, true), message("m1", 1, true)]);
        app.focus = Pane::Messages;

        assert_eq!(app.older_wanted(), None);
        app.update(Action::SelectNext);

        let before = Timestamp::from_second(86_400).unwrap();
        assert_eq!(app.older_wanted(), older(before, false));
    }

    #[test]
    fn a_folder_whose_mail_is_all_shown_fetches_nothing() {
        let mut app = devops(0, 1);
        app.set_messages(vec![message("m1", 1, true)]);

        assert_eq!(app.older_wanted(), None);
    }

    #[test]
    fn the_unread_list_fetches_older_unread_mail_by_the_unread_count() {
        let mut app = devops(50, 80);
        let row = app
            .folders
            .iter()
            .position(|f| f.id == unread_row(&FolderId("devops".to_owned())))
            .unwrap();
        app.select_folder(row);
        app.set_messages(Vec::new());

        let effect = app.older_wanted();

        assert!(matches!(
            effect,
            Some(Effect::LoadOlder {
                unread_only: true,
                ..
            })
        ));
    }

    #[test]
    fn a_folder_with_nothing_older_is_not_asked_again_until_the_next_sync() {
        let mut app = devops(0, 3);
        app.set_messages(Vec::new());
        app.older_wanted();

        app.older_loaded(FolderId("devops".to_owned()), false, false);

        assert_eq!(app.loading_older, None);
        assert_eq!(app.status, None);
        assert_eq!(app.older_wanted(), None);
        app.sync.pending = 1;
        app.sync_finished(std::time::Instant::now());
        assert!(app.older_wanted().is_some());
    }

    #[test]
    fn more_older_mail_is_asked_for_again() {
        let mut app = devops(0, 9);
        app.set_messages(vec![message("m1", 1, true)]);
        app.older_wanted();

        app.older_loaded(FolderId("devops".to_owned()), false, true);

        assert!(app.older_wanted().is_some());
    }
}
