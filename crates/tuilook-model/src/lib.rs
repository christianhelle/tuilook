//! Core domain types shared by every tuilook crate. No IO lives here.

mod action;
mod backend;
mod folder;
mod message;

pub use action::{Action, Event};
pub use backend::{BackendError, Body, BodyType, DeltaPage, MailBackend};
pub use folder::{Folder, FolderId, WellKnown, folder_depths, sort_folders};
pub use message::{Importance, Message, MessageId, sort_messages};
