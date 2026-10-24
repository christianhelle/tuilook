use std::future::Future;

use crate::{Folder, FolderId, Message, MessageId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// The delta link is no longer valid; the folder must be fully resynced.
    #[error("the folder's sync state expired")]
    DeltaExpired,
    #[error("{0}")]
    Other(String),
}

/// One page of changes from a folder's delta query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeltaPage {
    pub upserted: Vec<Message>,
    pub removed: Vec<MessageId>,
    /// Pass back on the next sync to receive only newer changes.
    pub delta_link: Option<String>,
}

/// A mail service. Graph is the only real implementation; tests use a fake.
pub trait MailBackend {
    fn list_folders(&self) -> impl Future<Output = Result<Vec<Folder>, BackendError>> + Send;

    fn delta_sync(
        &self,
        folder: &FolderId,
        delta_link: Option<&str>,
    ) -> impl Future<Output = Result<DeltaPage, BackendError>> + Send;
}
