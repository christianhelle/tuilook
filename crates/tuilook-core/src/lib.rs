//! Sync engine and command handlers. Core owns every write to the store.

mod body;
pub mod fake;
mod sync;

pub use body::body_text;
pub use sync::{SyncError, sync_folders, sync_messages};
