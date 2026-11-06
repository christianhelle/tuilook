//! The dialog shown over the panes, which takes every key until it closes.

use ratatui::crossterm::event::KeyEvent;

use crate::model::{Invite, MessageId};
use crate::tui::{
    account_manager::AccountManager,
    app::Confirm,
    attachments::{self, AttachmentPicker},
    files::{self, FilePicker},
    folder_edit::{self, NamePrompt},
    mover::{self, MovePicker},
};

/// At most one dialog is open at a time. A dialog owns its state and answers
/// each key with a request of its own, which the app acts on; it never
/// reaches into the app's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// The keys, until the next key or click.
    Help,
    /// What the mail service said when it blocked sending, until the next
    /// key or click.
    Blocked(String),
    /// A question to answer with y or Enter before anything else happens.
    Confirm(Confirm),
    /// A folder name being typed, to make or rename a folder.
    Name(NamePrompt),
    /// The folder being picked to move mail to.
    Move(MovePicker),
    /// The attached files being picked to save.
    Attachments(AttachmentPicker),
    /// The file being picked to attach.
    Files(FilePicker),
    /// The accounts, to add, sign in again, move or remove.
    Accounts(AccountManager),
    /// How to answer the invite in `message`, called `subject`.
    Invite {
        message: MessageId,
        subject: String,
        invite: Invite,
    },
}

impl Dialog {
    /// Whether text is typed into the dialog, at a cursor.
    pub fn takes_text(&self) -> bool {
        match self {
            Dialog::Name(_) | Dialog::Move(_) | Dialog::Files(_) => true,
            Dialog::Accounts(manager) => manager.takes_text(),
            _ => false,
        }
    }
}

/// A dialog that answers each key with a request of its own, and stays
/// open until it asks for something other than [`Picker::NONE`].
pub trait Picker: Sized {
    type Request: PartialEq;
    /// The request that leaves the dialog open, asking for nothing.
    const NONE: Self::Request;
    fn key(&mut self, key: KeyEvent) -> Self::Request;
    /// The dialog showing it.
    fn shown(self) -> Dialog;

    /// Hands `key` over: the dialog to keep open, unless the key closed
    /// it, and what it asks for.
    fn step(mut self, key: KeyEvent) -> (Option<Dialog>, Self::Request) {
        let request = Picker::key(&mut self, key);
        let open = (request == Self::NONE).then(|| self.shown());
        (open, request)
    }
}

macro_rules! picker {
    ($picker:ty, $module:ident, $variant:ident) => {
        impl Picker for $picker {
            type Request = $module::Request;
            const NONE: Self::Request = $module::Request::None;
            fn key(&mut self, key: KeyEvent) -> Self::Request {
                <$picker>::key(self, key)
            }
            fn shown(self) -> Dialog {
                Dialog::$variant(self)
            }
        }
    };
}

picker!(NamePrompt, folder_edit, Name);
picker!(MovePicker, mover, Move);
picker!(AttachmentPicker, attachments, Attachments);
picker!(FilePicker, files, Files);
