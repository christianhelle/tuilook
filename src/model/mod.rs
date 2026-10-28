//! Core domain types shared by every part of tuilook. No IO lives here.

mod action;
mod backend;
mod draft;
mod folder;
mod message;

pub use action::{Action, Event};
pub use backend::{BackendError, Body, BodyType, DeltaPage, MailBackend, OlderPage};
pub use draft::{ComposeKind, Draft, parse_addresses};
pub use folder::{Folder, FolderId, WellKnown, folder_depths, sort_folders};
pub use message::{Importance, Message, MessageChange, MessageId, sort_messages};
