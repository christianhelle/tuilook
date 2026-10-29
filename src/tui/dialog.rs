//! The dialog shown over the panes, which takes every key until it closes.

use crate::tui::{app::Confirm, folder_edit::NamePrompt, mover::MovePicker};

/// At most one dialog is open at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// A question to answer with y or Enter before anything else happens.
    Confirm(Confirm),
    /// A folder name being typed, to make or rename a folder.
    Name(NamePrompt),
    /// The folder being picked to move mail to.
    Move(MovePicker),
}
