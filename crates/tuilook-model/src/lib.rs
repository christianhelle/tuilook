//! Core domain types shared by every tuilook crate. No IO lives here.

mod action;
mod folder;
mod message;

pub use action::{Action, Event};
pub use folder::{Folder, FolderId, WellKnown, sort_folders};
pub use message::{Importance, Message, MessageId, sort_messages};
