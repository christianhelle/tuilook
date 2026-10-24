//! Core domain types shared by every tuilook crate. No IO lives here.

mod folder;
mod message;

pub use folder::{Folder, FolderId, WellKnown, sort_folders};
pub use message::{Importance, Message, MessageId, sort_messages};
