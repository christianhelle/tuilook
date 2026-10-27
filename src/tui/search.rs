//! Searching every cached folder: the query being typed, and the results
//! shown in the message list in place of a folder's messages.

use std::collections::{BTreeMap, HashMap};

use crate::model::{Message, MessageId};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{AppState, Effect, Pane};

/// The most results a search shows.
pub const SEARCH_LIMIT: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    pub query: String,
    /// Whether keys go to the query rather than the keymap.
    pub editing: bool,
    /// The account each result came from, once results replace the folder's
    /// messages; `None` while the folder is still shown.
    pub results: Option<HashMap<MessageId, usize>>,
}

impl AppState {
    /// `/`: starts typing a search, or goes back to the one shown.
    pub(crate) fn start_search(&mut self) {
        self.search.get_or_insert_with(Search::default).editing = true;
    }

    /// A key while typing the query. The results follow each change.
    pub(crate) fn search_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let search = self.search.as_mut()?;
        match key.code {
            KeyCode::Esc => self.leave_search(),
            KeyCode::Enter if search.query.trim().is_empty() => self.leave_search(),
            KeyCode::Enter => {
                search.editing = false;
                if search.results.is_some() {
                    self.focus = Pane::Messages;
                }
                None
            }
            KeyCode::Backspace => {
                search.query.pop();
                self.query_changed()
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                search.query.push(c);
                self.query_changed()
            }
            _ => None,
        }
    }

    fn query_changed(&mut self) -> Option<Effect> {
        let search = self.search.as_mut()?;
        if !search.query.trim().is_empty() {
            return Some(Effect::Search(search.query.clone()));
        }
        // Nothing to look for: show the folder again while typing goes on.
        search.results.take()?;
        self.shown_folder().map(Effect::LoadFolder)
    }

    /// Shows what a search found, with the account each came from, newest
    /// first. The selected message stays selected if it was found.
    pub fn search_results(&mut self, found: Vec<(usize, Message)>) -> Option<Effect> {
        let search = self.search.as_mut()?;
        let accounts = found.iter().map(|(a, m)| (m.id.clone(), *a)).collect();
        search.results = Some(accounts);
        self.refresh_messages(found.into_iter().map(|(_, m)| m).collect())
    }

    /// Drops the search, and shows the folder again if results replaced it.
    pub(crate) fn leave_search(&mut self) -> Option<Effect> {
        self.search.take()?.results?;
        self.shown_folder().map(Effect::LoadFolder)
    }

    /// The query whose results the message list shows, if it shows any.
    pub fn results_query(&self) -> Option<&str> {
        let search = self.search.as_ref()?;
        search.results.as_ref().map(|_| search.query.as_str())
    }

    /// Whether keys go to the search query.
    pub fn typing_search(&self) -> bool {
        self.search.as_ref().is_some_and(|s| s.editing)
    }

    /// The account a message shown is in: a result's own, or else the
    /// selected folder's.
    pub fn message_account(&self, id: &MessageId) -> usize {
        self.search
            .as_ref()
            .and_then(|s| s.results.as_ref()?.get(id).copied())
            .unwrap_or_else(|| self.current_account())
    }

    /// `ids` split by the account each message is in, keeping their order.
    pub fn by_account(&self, ids: Vec<MessageId>) -> BTreeMap<usize, Vec<MessageId>> {
        let mut split: BTreeMap<usize, Vec<MessageId>> = BTreeMap::new();
        for id in ids {
            split.entry(self.message_account(&id)).or_default().push(id);
        }
        split
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, Folder, FolderId};

    use super::*;
    use crate::tui::keymap::Keymap;

    fn folder(id: &str) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: None,
            name: id.to_owned(),
            well_known: None,
            unread: 0,
            total: 0,
        }
    }

    fn message(id: &str) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("a".to_owned()),
            conversation_id: String::new(),
            subject: id.to_owned(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: jiff::Timestamp::UNIX_EPOCH,
            is_read: true,
            flagged: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    fn press(app: &mut AppState, code: KeyCode) -> Option<Effect> {
        app.on_key(
            KeyEvent::new(code, KeyModifiers::NONE),
            &mut Keymap::default(),
        )
    }

    fn typed(app: &mut AppState, text: &str) -> Option<Effect> {
        text.chars()
            .map(|c| press(app, KeyCode::Char(c)))
            .last()
            .flatten()
    }

    /// Folders a and b, a selected and showing m1 and m2.
    fn in_folder() -> AppState {
        let mut app = AppState::default();
        app.set_folders(vec![folder("a"), folder("b")]);
        app.set_messages(vec![message("m1"), message("m2")]);
        app
    }

    fn load_folder(id: &str) -> Option<Effect> {
        Some(Effect::LoadFolder(FolderId(id.to_owned())))
    }

    /// A search for "x" that found r1 in account 0 and r2 in account 1.
    fn searched() -> AppState {
        let mut app = in_folder();
        app.update(Action::Search);
        typed(&mut app, "x");
        app.search_results(vec![(0, message("r1")), (1, message("r2"))]);
        app
    }

    #[test]
    fn slash_starts_typing_and_each_key_searches_again() {
        let mut app = in_folder();

        app.update(Action::Search);
        assert!(app.typing_search());

        assert_eq!(typed(&mut app, "lu"), Some(Effect::Search("lu".to_owned())));
        assert_eq!(
            press(&mut app, KeyCode::Backspace),
            Some(Effect::Search("l".to_owned()))
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('q')),
            Some(Effect::Search("lq".to_owned()))
        );
        assert!(!app.should_quit, "q is typed, not quitting");
    }

    #[test]
    fn results_replace_the_messages_until_the_search_is_left() {
        let mut app = searched();

        assert_eq!(app.results_query(), Some("x"));
        assert_eq!(app.messages, [message("r1"), message("r2")]);

        assert_eq!(press(&mut app, KeyCode::Esc), load_folder("a"));
        assert_eq!(app.search, None);
    }

    #[test]
    fn enter_keeps_the_results_and_moves_to_them() {
        let mut app = searched();

        assert_eq!(press(&mut app, KeyCode::Enter), None);

        assert!(!app.typing_search());
        assert_eq!(app.focus, Pane::Messages);
        assert_eq!(app.results_query(), Some("x"));
    }

    #[test]
    fn leaving_before_any_results_keeps_the_folder_as_it_is() {
        let mut app = in_folder();
        app.update(Action::Search);

        assert_eq!(press(&mut app, KeyCode::Esc), None);
        assert_eq!(app.search, None);
    }

    #[test]
    fn clearing_the_query_shows_the_folder_again() {
        let mut app = searched();

        assert_eq!(press(&mut app, KeyCode::Backspace), load_folder("a"));

        assert!(app.typing_search());
        assert_eq!(app.results_query(), None);
    }

    #[test]
    fn esc_in_the_results_clears_marks_then_leaves_the_search() {
        let mut app = searched();
        press(&mut app, KeyCode::Enter);
        app.update(Action::MarkAll);

        assert_eq!(press(&mut app, KeyCode::Esc), None);
        assert!(app.marked.is_empty());
        assert_eq!(press(&mut app, KeyCode::Esc), load_folder("a"));
        assert_eq!(app.search, None);
    }

    #[test]
    fn picking_a_folder_leaves_the_search() {
        let mut app = searched();
        press(&mut app, KeyCode::Enter);
        app.focus = Pane::Folders;

        assert_eq!(app.update(Action::SelectNext), load_folder("b"));
        assert_eq!(app.search, None);
    }

    #[test]
    fn picking_the_same_folder_shows_it_again() {
        let mut app = searched();

        assert_eq!(app.select_folder(0), load_folder("a"));
        assert_eq!(app.search, None);
    }

    #[test]
    fn a_result_is_read_and_answered_from_its_own_account() {
        let mut app = searched();
        let r2 = MessageId("r2".to_owned());

        assert_eq!(app.account_for(&Effect::LoadBody(r2.clone())), 1);
        assert_eq!(app.account_for(&Effect::MarkRead(r2)), 1);
        app.focus = Pane::Messages;
        app.update(Action::SelectNext);
        app.update(Action::Reply);
        assert_eq!(app.compose.map(|c| c.account), Some(1));
    }

    #[test]
    fn results_say_which_account_they_are_in() {
        let app = searched();
        let id = |id: &str| MessageId(id.to_owned());

        assert_eq!(app.message_account(&id("r2")), 1);
        assert_eq!(
            app.by_account(vec![id("r2"), id("r1")]),
            BTreeMap::from([(0, vec![id("r1")]), (1, vec![id("r2")])])
        );
    }
}
