//! Files attached to the message being read, and picking which to save.

use crate::model::{Attachment, MessageId};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::tui::{
    app::{AppState, Effect},
    dialog::Dialog,
};

/// What a key asks for while picking attachments to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Save these files of `message`, the mail the picker was opened on,
    /// whatever a sync has selected since.
    Save {
        message: MessageId,
        attachments: Vec<Attachment>,
    },
}

/// The attachments of a message, to pick which to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentPicker {
    pub message: MessageId,
    pub attachments: Vec<Attachment>,
    pub selected: usize,
}

impl AttachmentPicker {
    /// Up and down pick one, Enter saves it, a saves them all and Esc gives
    /// up.
    pub fn key(&mut self, key: KeyEvent) -> Request {
        let last = self.attachments.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Request::Close,
            KeyCode::Enter => {
                if let Some(picked) = self.attachments.get(self.selected) {
                    return Request::Save {
                        message: self.message.clone(),
                        attachments: vec![picked.clone()],
                    };
                }
            }
            KeyCode::Char('a') => {
                return Request::Save {
                    message: self.message.clone(),
                    attachments: self.attachments.clone(),
                };
            }
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(last);
            }
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            _ => {}
        }
        Request::None
    }
}

impl AppState {
    /// s: saves the selected message's attached file, or asks which of
    /// several to save.
    pub(crate) fn ask_which_to_save(&mut self) -> Option<Effect> {
        let message = self.selected()?.id.clone();
        match self.attachments.as_slice() {
            [] => {
                self.show_info("No files are attached to this mail");
                None
            }
            [only] => self.save_requested(Request::Save {
                message,
                attachments: vec![only.clone()],
            }),
            several => {
                self.dialog = Some(Dialog::Attachments(AttachmentPicker {
                    message,
                    attachments: several.to_vec(),
                    selected: 0,
                }));
                None
            }
        }
    }

    /// Saves the attachments the picker asked for, if it did.
    pub(crate) fn save_requested(&mut self, request: Request) -> Option<Effect> {
        let Request::Save {
            message,
            attachments,
        } = request
        else {
            return None;
        };
        let s = if attachments.len() == 1 { "" } else { "s" };
        self.show_busy(format!("Saving {} file{s}...", attachments.len()));
        Some(Effect::SaveAttachments {
            message,
            attachments,
        })
    }
}

/// `bytes` as a short size, such as `2 KB` or `1.2 MB`.
pub fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    match bytes {
        b if b < KB => format!("{b} B"),
        b if b < MB => format!("{} KB", b.div_ceil(KB)),
        b if b < GB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{:.1} GB", b as f64 / GB as f64),
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, Attachment, AttachmentId, AttachmentKind, Message, MessageId};
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    use crate::tui::{
        app::{AppState, Effect, Status},
        dialog::Dialog,
        keymap::Keymap,
    };

    fn attachment(id: &str) -> Attachment {
        Attachment {
            id: AttachmentId(id.to_owned()),
            name: format!("{id}.pdf"),
            content_type: String::new(),
            size: 10,
            kind: AttachmentKind::File,
        }
    }

    /// Reading m1, with `attachments` attached.
    fn reading(attachments: &[&str]) -> AppState {
        let mut app = AppState::default();
        app.set_messages(vec![Message {
            is_read: true,
            has_attachments: !attachments.is_empty(),
            ..crate::core::fake::message("m1")
        }]);
        let attachments = attachments.iter().map(|id| attachment(id)).collect();
        app.body_loaded(MessageId("m1".to_owned()), String::new(), attachments);
        app
    }

    fn press(app: &mut AppState, code: KeyCode) -> Option<Effect> {
        app.on_key(KeyEvent::from(code), &mut Keymap::default())
    }

    fn saving(ids: &[&str]) -> Option<Effect> {
        Some(Effect::SaveAttachments {
            message: MessageId("m1".to_owned()),
            attachments: ids.iter().map(|id| attachment(id)).collect(),
        })
    }

    #[test]
    fn s_saves_the_only_attachment_straight_away() {
        let mut app = reading(&["a1"]);

        assert_eq!(app.update(Action::SaveAttachments), saving(&["a1"]));
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn with_several_attachments_s_asks_which_and_enter_saves_the_picked_one() {
        let mut app = reading(&["a1", "a2", "a3"]);

        assert_eq!(app.update(Action::SaveAttachments), None);
        assert!(matches!(app.dialog, Some(Dialog::Attachments(_))));
        assert_eq!(press(&mut app, KeyCode::Char('j')), None);

        assert_eq!(press(&mut app, KeyCode::Enter), saving(&["a2"]));
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn a_in_the_picker_saves_them_all() {
        let mut app = reading(&["a1", "a2"]);
        app.update(Action::SaveAttachments);

        assert_eq!(press(&mut app, KeyCode::Char('a')), saving(&["a1", "a2"]));
    }

    #[test]
    fn the_picker_saves_from_its_message_even_if_a_sync_moves_the_selection() {
        let mut app = reading(&["a1", "a2"]);
        app.update(Action::SaveAttachments);
        let mut other = app.messages[0].clone();
        other.id = MessageId("m2".to_owned());

        app.refresh_messages(vec![other]);

        assert_eq!(press(&mut app, KeyCode::Enter), saving(&["a1"]));
    }

    #[test]
    fn esc_closes_the_picker_saving_nothing() {
        let mut app = reading(&["a1", "a2"]);
        app.update(Action::SaveAttachments);

        assert_eq!(press(&mut app, KeyCode::Esc), None);
        assert_eq!(app.dialog, None);
    }

    #[test]
    fn s_on_mail_without_attachments_says_so() {
        let mut app = reading(&[]);

        assert_eq!(app.update(Action::SaveAttachments), None);
        assert_eq!(app.dialog, None);
        assert_eq!(
            app.status,
            Some(Status::Info(
                "No files are attached to this mail".to_owned()
            ))
        );
    }
}
