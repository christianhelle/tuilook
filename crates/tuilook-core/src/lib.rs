//! Sync engine and command handlers. Core owns every write to the store.

pub mod fake;
mod sync;

pub use sync::{SyncError, sync_folders};
