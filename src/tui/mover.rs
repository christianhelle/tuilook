//! Moving mail: picking the folder to move the marked or selected messages
//! to, narrowed by typing part of its name.

use crate::model::{Folder, FolderId, MessageId, folder_depths, sort_folders};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{AppState, Effect, Pane};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePicker {
    /// The account the messages are in, and so the folders to pick from.
    pub account: usize,
    pub ids: Vec<MessageId>,
    /// The account's folders in tree order, each with how deeply it is
    /// nested.
    pub folders: Vec<(Folder, usize)>,
    /// Part of a folder's name, to narrow the folders shown.
    pub filter: String,
    /// The picked one of the folders shown.
    pub selected: usize,
}

impl MovePicker {
    /// The folders whose name has the filter in it, as indexes into
    /// `folders`.
    pub fn choices(&self) -> Vec<usize> {
        let filter = self.filter.trim().to_lowercase();
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
        let mut folders = self
            .account_folders
            .get(account)
            .and_then(Option::clone)
            .unwrap_or_default();
        sort_folders(&mut folders);
        let depths = folder_depths(&folders);
        let folders = folders
            .into_iter()
            .zip(depths)
            .filter(|(f, _)| Some(&f.id) != all_in)
            .collect();
        self.mover = Some(MovePicker {
            account,
            ids,
            folders,
            filter: String::new(),
            selected: 0,
        });
    }

    /// A key while picking where to move mail. Typing narrows the folders,
    /// ↑ ↓ pick one, Enter moves the mail there and Esc gives up.
    pub(crate) fn mover_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let mover = self.mover.as_mut()?;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.mover = None,
            KeyCode::Enter => {
                let to = mover.picked()?.clone();
                let MovePicker { account, ids, .. } = self.mover.take()?;
                let s = if ids.len() == 1 { "" } else { "s" };
                self.show_busy(format!("Moving {} message{s} to {}…", ids.len(), to.name));
                return Some(Effect::Move {
                    account,
                    ids,
                    to: to.id,
                });
            }
            KeyCode::Down | KeyCode::Tab => step(mover, 1),
            KeyCode::Char('n') if ctrl => step(mover, 1),
            KeyCode::Up | KeyCode::BackTab => step(mover, -1),
            KeyCode::Char('p') if ctrl => step(mover, -1),
            KeyCode::Backspace => {
                mover.filter.pop();
                mover.selected = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                mover.filter.push(c);
                mover.selected = 0;
            }
            _ => {}
        }
        None
    }
}

/// Picks the folder `delta` rows away, stopping at the ends.
fn step(mover: &mut MovePicker, delta: isize) {
    let last = mover.choices().len().saturating_sub(1);
    mover.selected = mover.selected.saturating_add_signed(delta).min(last);
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

    fn names(app: &AppState) -> Vec<&str> {
        let mover = app.mover.as_ref().unwrap();
        mover
            .choices()
            .into_iter()
            .map(|i| mover.folders[i].0.name.as_str())
            .collect()
    }

    fn type_keys(app: &mut AppState, keys: &[KeyCode]) -> Option<Effect> {
        let mut keymap = Keymap::default();
        let mut effect = None;
        for &code in keys {
            effect = app.on_key(KeyEvent::from(code), &mut keymap);
        }
        effect
    }

    fn chars(text: &str) -> Vec<KeyCode> {
        text.chars().map(KeyCode::Char).collect()
    }

    #[test]
    fn m_offers_every_other_folder_of_the_account() {
        let mut app = in_inbox();

        app.update(Action::Move);

        assert_eq!(names(&app), ["Archive", "Receipts"]);
        assert_eq!(
            app.mover.as_ref().unwrap().ids,
            [MessageId("m1".to_owned())]
        );
    }

    #[test]
    fn typing_narrows_the_folders_and_enter_moves_the_marked_mail() {
        let mut app = in_inbox();
        app.update(Action::MarkAll);

        app.update(Action::Move);
        type_keys(&mut app, &chars("arch"));
        assert_eq!(names(&app), ["Archive"]);
        let effect = type_keys(&mut app, &[KeyCode::Enter]);

        assert_eq!(
            effect,
            Some(Effect::Move {
                account: 0,
                ids: vec![MessageId("m1".to_owned()), MessageId("m2".to_owned())],
                to: FolderId("Archive".to_owned()),
            })
        );
        assert_eq!(app.mover, None);
    }

    #[test]
    fn arrows_pick_a_folder_and_stop_at_the_ends() {
        let mut app = in_inbox();

        app.update(Action::Move);
        type_keys(&mut app, &[KeyCode::Down, KeyCode::Down, KeyCode::Down]);
        assert_eq!(
            app.mover.as_ref().unwrap().picked().unwrap().name,
            "Receipts"
        );
        type_keys(&mut app, &[KeyCode::Up, KeyCode::Up]);

        assert_eq!(
            app.mover.as_ref().unwrap().picked().unwrap().name,
            "Archive"
        );
    }

    #[test]
    fn esc_gives_up_and_enter_with_nothing_shown_does_nothing() {
        let mut app = in_inbox();

        app.update(Action::Move);
        let effect = type_keys(&mut app, &[chars("zzz"), vec![KeyCode::Enter]].concat());
        assert_eq!(effect, None);
        assert!(app.mover.is_some());
        type_keys(&mut app, &[KeyCode::Esc]);

        assert_eq!(app.mover, None);
    }

    #[test]
    fn mail_is_not_moved_from_the_folder_pane() {
        let mut app = in_inbox();
        app.focus = Pane::Folders;

        app.update(Action::Move);

        assert_eq!(app.mover, None);
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
