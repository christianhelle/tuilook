//! What fills the screen, and so takes the keys, under any dialog.

use crate::tui::{compose::Compose, signature::SignatureEditor};

/// One screen at a time: which one is the whole of what the app needs to know
/// to route a key or draw a frame.
#[derive(Debug, Default)]
pub enum Screen {
    /// The folders, messages and reading pane.
    #[default]
    Panes,
    /// A message being written.
    Compose(Box<Compose>),
    /// An account's signature being written.
    Signature(Box<SignatureEditor>),
}

impl Screen {
    /// The message being written, if one is.
    pub fn compose(&self) -> Option<&Compose> {
        match self {
            Screen::Compose(compose) => Some(compose.as_ref()),
            _ => None,
        }
    }

    pub fn compose_mut(&mut self) -> Option<&mut Compose> {
        match self {
            Screen::Compose(compose) => Some(compose.as_mut()),
            _ => None,
        }
    }

    /// The signature being written, if one is.
    pub fn signature(&self) -> Option<&SignatureEditor> {
        match self {
            Screen::Signature(editor) => Some(editor.as_ref()),
            _ => None,
        }
    }

    pub fn signature_mut(&mut self) -> Option<&mut SignatureEditor> {
        match self {
            Screen::Signature(editor) => Some(editor.as_mut()),
            _ => None,
        }
    }

    /// Back to the panes, if a signature is being written.
    pub fn close_signature(&mut self) {
        if self.signature().is_some() {
            *self = Screen::Panes;
        }
    }

    /// Back to the panes, if a message is being written. A send that
    /// finishes after the user has moved on leaves the other screens alone.
    pub fn close_compose(&mut self) {
        if self.compose().is_some() {
            *self = Screen::Panes;
        }
    }
}
