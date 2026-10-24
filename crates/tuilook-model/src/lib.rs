//! Core domain types shared by every tuilook crate. No IO lives here.

mod folder;

pub use folder::{Folder, FolderId, WellKnown, sort_folders};
