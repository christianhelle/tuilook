//! Making and renaming folders: the name being typed for one.

use crate::model::{Folder, FolderId};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::tui::{
    app::{AppState, Effect, is_account_row, row_target},
    dialog::Dialog,
    input::TextInput,
};

/// What the name being typed is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    /// A new folder inside `parent`, called `under`, or at the top level of
    /// the account.
    New {
        parent: Option<FolderId>,
        under: Option<String>,
    },
    /// A new name for a folder now called `old`.
    Rename { folder: FolderId, old: String },
}

/// What a key asks for while typing a folder name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Make a folder called `name` in `account`, inside `parent` or at the
    /// top level.
    Create {
        account: usize,
        parent: Option<FolderId>,
        name: String,
    },
    /// Rename `folder` in `account` from `old` to `name`.
    Rename {
        account: usize,
        folder: FolderId,
        old: String,
        name: String,
    },
}

/// A folder name being typed, shown in a dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePrompt {
    pub account: usize,
    pub naming: Naming,
    pub name: TextInput,
}

impl NamePrompt {
    /// Asks the name of a new folder in `account`, inside `parent`, called
    /// `under`, or at the top level without one.
    pub fn new_folder(account: usize, parent: Option<FolderId>, under: Option<String>) -> Self {
        Self {
            account,
            naming: Naming::New { parent, under },
            name: TextInput::default(),
        }
    }

    /// Asks a new name for `folder` in `account`, starting from the one it
    /// has.
    pub fn rename(account: usize, folder: &Folder) -> Self {
        Self {
            account,
            naming: Naming::Rename {
                folder: folder.id.clone(),
                old: folder.name.clone(),
            },
            name: TextInput::from(folder.name.as_str()),
        }
    }

    /// Typing goes into the name, Enter makes or renames the folder and Esc
    /// gives up. A blank name keeps asking; the same name renames nothing.
    pub fn key(&mut self, key: KeyEvent) -> Request {
        match key.code {
            KeyCode::Esc => return Request::Close,
            KeyCode::Enter => {
                let name = self.name.text().trim().to_owned();
                if name.is_empty() {
                    return Request::None;
                }
                let account = self.account;
                return match &self.naming {
                    Naming::New { parent, .. } => Request::Create {
                        account,
                        parent: parent.clone(),
                        name,
                    },
                    Naming::Rename { old, .. } if *old == name => Request::Close,
                    Naming::Rename { folder, old } => Request::Rename {
                        account,
                        folder: folder.clone(),
                        old: old.clone(),
                        name,
                    },
                };
            }
            _ => {
                self.name.key(key);
            }
        }
        Request::None
    }

    /// What the dialog asks for.
    pub fn title(&self) -> String {
        match &self.naming {
            Naming::New {
                under: Some(under), ..
            } => format!("New folder in {under}"),
            Naming::New { under: None, .. } => "New folder".to_owned(),
            Naming::Rename { old, .. } => format!("Rename {old}"),
        }
    }
}

impl AppState {
    /// a or A in the folder pane: asks the name of a new folder, inside the
    /// selected one or, with `top_level` or on an account's row, at the top
    /// level of the selected folder's account.
    pub(crate) fn ask_folder_name(&mut self, top_level: bool) {
        let Some(row) = self.folders.get(self.selected_folder) else {
            return;
        };
        let account = self.current_account();
        let parent = (!top_level && !is_account_row(row)).then(|| row_target(&row.id));
        let under = parent
            .as_ref()
            .and_then(|id| self.real_folder(id))
            .map(|f| f.name.clone());
        self.dialog = Some(Dialog::Name(NamePrompt::new_folder(account, parent, under)));
    }

    /// F2 in the folder pane: asks a new name for the selected folder,
    /// starting from the one it has. Outlook's own folders keep theirs.
    pub(crate) fn ask_new_name(&mut self) {
        let Some(row) = self.folders.get(self.selected_folder) else {
            return;
        };
        if is_account_row(row) {
            return;
        }
        let account = self.current_account();
        let Some(folder) = self.real_folder(&row_target(&row.id)) else {
            return;
        };
        if folder.well_known.is_some() {
            let old = &folder.name;
            self.show_error(format!(
                "{old} is one of Outlook's own folders and cannot be renamed"
            ));
            return;
        }
        self.dialog = Some(Dialog::Name(NamePrompt::rename(account, folder)));
    }

    /// Makes or renames the folder the name prompt asked for, if it did.
    pub(crate) fn name_requested(&mut self, request: Request) -> Option<Effect> {
        match request {
            Request::None | Request::Close => None,
            Request::Create {
                account,
                parent,
                name,
            } => {
                self.show_busy(format!("Creating {name}…"));
                Some(Effect::CreateFolder {
                    account,
                    parent,
                    name,
                })
            }
            Request::Rename {
                account,
                folder,
                old,
                name,
            } => {
                self.show_busy(format!("Renaming {old}…"));
                Some(Effect::RenameFolder {
                    account,
                    folder,
                    name,
                })
            }
        }
    }

    /// The folder `id` itself, not a favorite or unread row standing for it.
    pub(crate) fn real_folder(&self, id: &FolderId) -> Option<&Folder> {
        self.folders.iter().find(|f| &f.id == id)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, WellKnown};

    use super::*;
    use crate::tui::{app::Pane, keymap::Keymap};
    use ratatui::crossterm::event::KeyModifiers;

    fn folder(id: &str, parent: Option<&str>) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: parent.map(|p| FolderId(p.to_owned())),
            name: id.to_owned(),
            well_known: (id == "Inbox").then_some(WellKnown::Inbox),
            unread: 0,
            total: 0,
        }
    }

    /// Inbox > Receipts, with Receipts selected in the folder pane.
    fn on_receipts() -> AppState {
        let mut app = AppState::default();
        app.set_folders(vec![
            folder("Inbox", None),
            folder("Receipts", Some("Inbox")),
        ]);
        app.expanded.insert(FolderId("Inbox".to_owned()));
        app.selected_folder = 1;
        app.focus = Pane::Folders;
        app
    }

    fn type_text(app: &mut AppState, text: &str) -> Option<Effect> {
        let mut keymap = Keymap::default();
        let mut effect = None;
        for c in text.chars() {
            let code = match c {
                '\n' => KeyCode::Enter,
                '\u{8}' => KeyCode::Backspace,
                '\u{1b}' => KeyCode::Esc,
                c => KeyCode::Char(c),
            };
            effect = app.on_key(KeyEvent::new(code, KeyModifiers::NONE), &mut keymap);
        }
        effect
    }

    fn prompt(app: &AppState) -> &NamePrompt {
        match &app.dialog {
            Some(Dialog::Name(prompt)) => prompt,
            dialog => panic!("no name prompt but {dialog:?}"),
        }
    }

    fn type_into(prompt: &mut NamePrompt, text: &str) -> Request {
        let mut request = Request::None;
        for c in text.chars() {
            let code = match c {
                '\n' => KeyCode::Enter,
                '\u{8}' => KeyCode::Backspace,
                '\u{1b}' => KeyCode::Esc,
                c => KeyCode::Char(c),
            };
            request = prompt.key(KeyEvent::from(code));
        }
        request
    }

    #[test]
    fn a_asks_a_name_and_enter_makes_the_folder_inside_the_selected_one() {
        let mut app = on_receipts();

        app.update(Action::AddFolder);
        assert_eq!(prompt(&app).title(), "New folder in Receipts");
        let effect = type_text(&mut app, "2026\n");

        assert_eq!(
            effect,
            Some(Effect::CreateFolder {
                account: 0,
                parent: Some(FolderId("Receipts".to_owned())),
                name: "2026".to_owned(),
            })
        );
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn shift_a_makes_a_top_level_folder() {
        let mut app = on_receipts();

        app.update(Action::AddTopFolder);
        assert_eq!(prompt(&app).title(), "New folder");
        let effect = type_text(&mut app, "Travel\n");

        assert_eq!(
            effect,
            Some(Effect::CreateFolder {
                account: 0,
                parent: None,
                name: "Travel".to_owned(),
            })
        );
    }

    #[test]
    fn keys_go_to_the_name_and_esc_gives_up() {
        let mut app = on_receipts();

        app.update(Action::AddFolder);
        type_text(&mut app, "q");
        assert_eq!(prompt(&app).name.text(), "q");
        assert!(!app.should_quit, "q is typed, not a quit");
        let effect = type_text(&mut app, "\u{1b}");

        assert_eq!(effect, None);
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn a_blank_name_keeps_asking() {
        let mut prompt = NamePrompt::new_folder(0, None, None);

        assert_eq!(type_into(&mut prompt, "  \n"), Request::None);
    }

    #[test]
    fn backspace_takes_back_a_letter_and_esc_gives_up() {
        let mut prompt = NamePrompt::new_folder(0, None, None);

        type_into(&mut prompt, "qj\u{8}");
        assert_eq!(prompt.name.text(), "q");

        assert_eq!(type_into(&mut prompt, "\u{1b}"), Request::Close);
    }

    #[test]
    fn f2_starts_from_the_folders_name_and_renames_it() {
        let mut app = on_receipts();

        app.update(Action::RenameFolder);
        assert_eq!(prompt(&app).name.text(), "Receipts");
        let effect = type_text(&mut app, "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}Bills\n");

        assert_eq!(
            effect,
            Some(Effect::RenameFolder {
                account: 0,
                folder: FolderId("Receipts".to_owned()),
                name: "Bills".to_owned(),
            })
        );
    }

    #[test]
    fn arrows_move_the_cursor_to_type_inside_the_name() {
        let mut prompt = NamePrompt::rename(0, &folder("Bils", None));
        prompt.key(KeyEvent::from(KeyCode::Left));
        prompt.key(KeyEvent::from(KeyCode::Left));

        let request = type_into(
            &mut prompt,
            "l
",
        );

        assert_eq!(
            request,
            Request::Rename {
                account: 0,
                folder: FolderId("Bils".to_owned()),
                old: "Bils".to_owned(),
                name: "Bills".to_owned(),
            }
        );
    }

    #[test]
    fn keeping_the_same_name_renames_nothing() {
        let mut prompt = NamePrompt::rename(0, &folder("Receipts", Some("Inbox")));

        assert_eq!(type_into(&mut prompt, "\n"), Request::Close);
    }

    #[test]
    fn outlooks_own_folders_cannot_be_renamed() {
        let mut app = on_receipts();
        app.selected_folder = 0;

        app.update(Action::RenameFolder);

        assert_eq!(app.dialog, None);
        assert!(app.status.is_some());
    }

    #[test]
    fn folders_are_made_and_renamed_only_from_the_folder_pane() {
        let mut app = on_receipts();
        app.focus = Pane::Messages;

        app.update(Action::AddFolder);
        app.update(Action::RenameFolder);

        assert_eq!(app.dialog, None);
    }
}
