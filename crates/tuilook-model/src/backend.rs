use std::future::Future;

use crate::{Folder, FolderId, Message, MessageId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct BackendError(pub String);

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
