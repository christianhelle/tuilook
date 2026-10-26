//! Where the user was when they quit, to pick up there next time.

use std::{io, path::Path};

use crate::model::{FolderId, MessageId};
use serde::{Deserialize, Serialize};

use crate::tui::app::{AppState, Effect, Pane};

/// What is kept between runs. Anything missing or unknown is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub focus: Pane,
    pub folder: Option<String>,
    pub message: Option<String>,
    pub zoomed: bool,
    pub folders_hidden: bool,
    /// Folders whose subfolders were shown.
    pub expanded: Vec<String>,
}

impl Snapshot {
    pub fn of(app: &AppState) -> Self {
        Snapshot {
            focus: app.focus,
            folder: app.current_folder().map(|id| id.0.clone()),
            message: app
                .messages
                .get(app.selected_message)
                .map(|m| m.id.0.clone()),
            zoomed: app.zoomed,
            folders_hidden: app.folders_hidden,
            expanded: app.expanded.iter().map(|id| id.0.clone()).collect(),
        }
    }
}

/// The snapshot saved at `path`, if there is a readable one.
pub fn load(path: &Path) -> Option<Snapshot> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let text = serde_json::to_string_pretty(snapshot).map_err(io::Error::other)?;
    std::fs::write(path, text)
}

impl AppState {
    /// Goes back to the panes and folder of `snapshot`. Load the folder this
    /// asks for, then call [`AppState::select_message`] for the message.
    pub fn restore(&mut self, snapshot: &Snapshot) -> Option<Effect> {
        self.zoomed = snapshot.zoomed;
        self.folders_hidden = snapshot.folders_hidden;
        self.focus = match snapshot.focus {
            Pane::Folders if snapshot.folders_hidden => Pane::Messages,
            focus => focus,
        };
        self.expanded
            .extend(snapshot.expanded.iter().cloned().map(FolderId));
        let id = FolderId(snapshot.folder.clone()?);
        let index = self.folders.iter().position(|f| f.id == id)?;
        let effect = self.select_folder(index);
        self.reveal_selected_folder();
        effect
    }

    /// Selects a message in the list by id, if it is there.
    pub fn select_message(&mut self, id: &str) -> Option<Effect> {
        let id = MessageId(id.to_owned());
        let index = self.messages.iter().position(|m| m.id == id)?;
        if index == self.selected_message {
            return None;
        }
        self.selected_message = index;
        Some(Effect::LoadBody(id))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Folder, Message};

    use super::*;

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
            folder_id: FolderId("b".to_owned()),
            conversation_id: String::new(),
            subject: String::new(),
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

    #[test]
    fn a_snapshot_brings_back_the_folder_message_and_panes() {
        let snapshot = Snapshot {
            focus: Pane::Reader,
            folder: Some("b".to_owned()),
            message: Some("m2".to_owned()),
            zoomed: true,
            folders_hidden: true,
            expanded: vec!["a".to_owned()],
        };
        let mut app = AppState::default();
        let mut b = folder("b");
        b.parent_id = Some(FolderId("a".to_owned()));
        app.set_folders(vec![folder("a"), b]);

        let effect = app.restore(&snapshot);
        assert_eq!(effect, Some(Effect::LoadFolder(FolderId("b".to_owned()))));
        app.set_messages(vec![message("m1"), message("m2")]);
        let effect = app.select_message("m2");

        assert_eq!(effect, Some(Effect::LoadBody(MessageId("m2".to_owned()))));
        assert_eq!(Snapshot::of(&app), snapshot);
    }

    #[test]
    fn the_restored_folder_is_shown_even_inside_a_collapsed_one() {
        let mut app = AppState::default();
        let mut b = folder("b");
        b.parent_id = Some(FolderId("a".to_owned()));
        app.set_folders(vec![folder("a"), b, folder("c")]);

        app.restore(&Snapshot {
            folder: Some("b".to_owned()),
            ..Default::default()
        });

        assert_eq!(app.visible_folders(), [0, 1, 2]);
        assert_eq!(app.selected_folder, 1);
    }

    #[test]
    fn folders_and_messages_that_are_gone_are_skipped() {
        let mut app = AppState::default();
        app.set_folders(vec![folder("a")]);
        let snapshot = Snapshot {
            folder: Some("gone".to_owned()),
            ..Default::default()
        };

        assert_eq!(app.restore(&snapshot), None);
        assert_eq!(app.select_message("gone"), None);
        assert_eq!(app.selected_folder, 0);
    }

    #[test]
    fn hidden_folders_cannot_keep_the_focus() {
        let mut app = AppState::default();

        app.restore(&Snapshot {
            focus: Pane::Folders,
            folders_hidden: true,
            ..Default::default()
        });

        assert_eq!(app.focus, Pane::Messages);
    }

    #[test]
    fn a_snapshot_survives_the_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("tuilook-session-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        let snapshot = Snapshot {
            folder: Some("inbox".to_owned()),
            zoomed: true,
            ..Default::default()
        };

        save(&path, &snapshot).unwrap();

        assert_eq!(load(&path), Some(snapshot));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
