//! Moving mail: picking the folder to move the marked or selected messages
//! to, narrowed by typing part of its name.

use crate::model::{Folder, FolderId, MessageId, folder_depths, sort_folders};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{
    app::{AppState, Effect, Pane},
    dialog::Dialog,
    input::TextInput,
};

/// What a key asks for while picking where to move mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Move `ids` in `account` to `to`.
    Move {
        account: usize,
        ids: Vec<MessageId>,
        to: Folder,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePicker {
    /// The account the messages are in, and so the folders to pick from.
    pub account: usize,
    pub ids: Vec<MessageId>,
    /// The account's folders in tree order, each with how deeply it is
    /// nested.
    pub folders: Vec<(Folder, usize)>,
    /// Part of a folder's name, to narrow the folders shown.
    pub filter: TextInput,
    /// The picked one of the folders shown.
    pub selected: usize,
}

impl MovePicker {
    /// Picks where to move `ids` among the account's `folders`, leaving out
    /// `all_in`, the folder they are all in already, if any.
    pub fn new(
        account: usize,
        ids: Vec<MessageId>,
        mut folders: Vec<Folder>,
        all_in: Option<&FolderId>,
    ) -> Self {
        sort_folders(&mut folders);
        let depths = folder_depths(&folders);
        let folders = folders
            .into_iter()
            .zip(depths)
            .filter(|(f, _)| Some(&f.id) != all_in)
            .collect();
        Self {
            account,
            ids,
            folders,
            filter: TextInput::default(),
            selected: 0,
        }
    }

    /// The folders whose name has the filter in it, as indexes into
    /// `folders`.
    pub fn choices(&self) -> Vec<usize> {
        let filter = self.filter.text().trim().to_lowercase();
        (0..self.folders.len())
            .filter(|&i| self.folders[i].0.name.to_lowercase().contains(&filter))
            .collect()
    }

    /// The folder picked, if any is shown.
    pub fn picked(&self) -> Option<&Folder> {
        let index = *self.choices().get(self.selected)?;
        Some(&self.folders[index].0)
    }

    /// What the dialog asks.
    pub fn title(&self) -> String {
        match self.ids.len() {
            1 => "Move 1 message to".to_owned(),
            n => format!("Move {n} messages to"),
        }
    }

    /// Typing narrows the folders, ↑ ↓ pick one, Enter moves the mail there
    /// and Esc gives up.
    pub fn key(&mut self, key: KeyEvent) -> Request {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Request::Close,
            KeyCode::Enter => {
                if let Some(to) = self.picked() {
                    return Request::Move {
                        account: self.account,
                        ids: self.ids.clone(),
                        to: to.clone(),
                    };
                }
            }
            KeyCode::Down | KeyCode::Tab => self.step(1),
            KeyCode::Char('n') if ctrl => self.step(1),
            KeyCode::Up | KeyCode::BackTab => self.step(-1),
            KeyCode::Char('p') if ctrl => self.step(-1),
            _ => {
                let before = self.filter.text().to_owned();
                if self.filter.key(key) && self.filter.text() != before {
                    self.selected = 0;
                }
            }
        }
        Request::None
    }

    /// Picks the folder `delta` rows away, stopping at the ends.
    fn step(&mut self, delta: isize) {
        let last = self.choices().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }
}

impl AppState {
    /// m outside the folder pane: asks which folder to move the marked or
    /// selected messages to. They must all be in one account.
    pub(crate) fn ask_where_to_move(&mut self) {
        if self.focus == Pane::Folders {
            return;
        }
        let ids = self.to_act_on();
        let accounts = self.by_account(ids.clone());
        let mut accounts = accounts.keys();
        let (Some(&account), None) = (accounts.next(), accounts.next()) else {
            if !ids.is_empty() {
                self.show_error(
                    "Marked mail is in more than one account; move it one at a time".to_owned(),
                );
            }
            return;
        };
        // Moving mail to the folder it is all in already does nothing.
        let mut from = ids.iter().map(|id| {
            self.messages
                .iter()
                .find(|m| &m.id == id)
                .map(|m| &m.folder_id)
        });
        let first = from.next().flatten();
        let all_in: Option<&FolderId> = from.all(|f| f == first).then_some(first).flatten();
        let folders = self
            .account_folders
            .get(account)
            .and_then(Option::clone)
            .unwrap_or_default();
        self.dialog = Some(Dialog::Move(MovePicker::new(account, ids, folders, all_in)));
    }

    /// Moves the mail where the move picker asked, if it did.
    pub(crate) fn move_requested(&mut self, request: Request) -> Option<Effect> {
        match request {
            Request::None | Request::Close => None,
            Request::Move { account, ids, to } => {
                let s = if ids.len() == 1 { "" } else { "s" };
                self.show_busy(format!("Moving {} message{s} to {}…", ids.len(), to.name));
                Some(Effect::Move {
                    account,
                    ids,
                    to: to.id,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, Message};
    use ratatui::crossterm::event::KeyEvent;

    use super::*;
    use crate::tui::keymap::Keymap;

    fn folder(id: &str, parent: Option<&str>) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: parent.map(|p| FolderId(p.to_owned())),
            name: id.to_owned(),
            well_known: None,
            unread: 0,
            total: 0,
        }
    }

    fn message(id: &str) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("Inbox".to_owned()),
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

    /// Archive, then Inbox > Receipts, with Inbox's m1 and m2 listed.
    fn in_inbox() -> AppState {
        let mut app = AppState::default();
        app.set_account_folders(
            0,
            vec![
                folder("Inbox", None),
                folder("Receipts", Some("Inbox")),
                folder("Archive", None),
            ],
        );
        app.set_messages(vec![message("m1"), message("m2")]);
        app.focus = Pane::Messages;
        app
    }

    fn names(mover: &MovePicker) -> Vec<&str> {
        mover
            .choices()
            .into_iter()
            .map(|i| mover.folders[i].0.name.as_str())
            .collect()
    }

    /// Picks where to move m1 among Archive and Inbox > Receipts.
    fn picker() -> MovePicker {
        let folders = vec![
            folder("Inbox", None),
            folder("Receipts", Some("Inbox")),
            folder("Archive", None),
        ];
        let ids = vec![MessageId("m1".to_owned())];
        MovePicker::new(0, ids, folders, Some(&FolderId("Inbox".to_owned())))
    }

    fn type_keys(mover: &mut MovePicker, keys: &[KeyCode]) -> Request {
        let mut request = Request::None;
        for &code in keys {
            request = mover.key(KeyEvent::from(code));
        }
        request
    }

    fn chars(text: &str) -> Vec<KeyCode> {
        text.chars().map(KeyCode::Char).collect()
    }

    #[test]
    fn m_offers_every_other_folder_of_the_account() {
        let mut app = in_inbox();

        app.update(Action::Move);

        let Some(Dialog::Move(mover)) = &app.dialog else {
            panic!("no move picker");
        };
        assert_eq!(names(mover), ["Archive", "Receipts"]);
        assert_eq!(mover.ids, [MessageId("m1".to_owned())]);
    }

    #[test]
    fn enter_moves_the_marked_mail_to_the_folder_picked() {
        let mut app = in_inbox();
        let mut keymap = Keymap::default();
        app.update(Action::MarkAll);

        app.update(Action::Move);
        for c in "arch".chars() {
            app.on_key(KeyEvent::from(KeyCode::Char(c)), &mut keymap);
        }
        let effect = app.on_key(KeyEvent::from(KeyCode::Enter), &mut keymap);

        assert_eq!(
            effect,
            Some(Effect::Move {
                account: 0,
                ids: vec![MessageId("m1".to_owned()), MessageId("m2".to_owned())],
                to: FolderId("Archive".to_owned()),
            })
        );
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn typing_narrows_the_folders() {
        let mut mover = picker();

        type_keys(&mut mover, &chars("arch"));

        assert_eq!(names(&mover), ["Archive"]);
    }

    #[test]
    fn typing_inside_the_filter_narrows_by_the_whole_of_it() {
        let mut mover = picker();

        type_keys(&mut mover, &chars("rc"));
        type_keys(&mut mover, &[KeyCode::Left, KeyCode::Char('e')]);

        assert_eq!(mover.filter.text(), "rec");
        assert_eq!(names(&mover), ["Receipts"]);
    }

    #[test]
    fn arrows_pick_a_folder_and_stop_at_the_ends() {
        let mut mover = picker();

        type_keys(&mut mover, &[KeyCode::Down, KeyCode::Down, KeyCode::Down]);
        assert_eq!(mover.picked().unwrap().name, "Receipts");
        type_keys(&mut mover, &[KeyCode::Up, KeyCode::Up]);

        assert_eq!(mover.picked().unwrap().name, "Archive");
    }

    #[test]
    fn esc_gives_up_and_enter_with_nothing_shown_does_nothing() {
        let mut mover = picker();

        let request = type_keys(&mut mover, &[chars("zzz"), vec![KeyCode::Enter]].concat());
        assert_eq!(request, Request::None);

        assert_eq!(type_keys(&mut mover, &[KeyCode::Esc]), Request::Close);
    }

    #[test]
    fn mail_is_not_moved_from_the_folder_pane() {
        let mut app = in_inbox();
        app.focus = Pane::Folders;

        app.update(Action::Move);

        assert_eq!(app.dialog, None);
    }

    #[test]
    fn moved_mail_leaves_the_list() {
        let mut app = in_inbox();

        app.messages_moved(
            &[MessageId("m1".to_owned())],
            &FolderId("Archive".to_owned()),
        );

        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.selected().unwrap().id, MessageId("m2".to_owned()));
    }
}
