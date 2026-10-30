//! Sync engine and command handlers. Core owns every write to the store.

mod body;
mod calendar;
mod engine;
pub mod fake;
mod sync;

pub use body::body_text;
pub use calendar::calendar_window;
pub use engine::{Command, run_engine};
pub use sync::{SyncError, load_body, sync_folders, sync_messages};
