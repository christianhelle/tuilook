use std::future::Future;

use crate::model::{Draft, Folder, FolderId, Message, MessageChange, MessageId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// The delta link is no longer valid; the folder must be fully resynced.
    #[error("the folder's sync state expired")]
    DeltaExpired,
    /// The mail service refuses to send from this account, usually because
    /// it took recent sending for spam. Holds what the service said.
    #[error("Outlook.com has blocked sending from this account ({0})")]
    SendingBlocked(String),
    #[error("{0}")]
    Other(String),
}

/// One page of changes from a folder's delta query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeltaPage {
    /// Messages new to the folder, or sent whole.
    pub upserted: Vec<Message>,
    /// Messages that changed, with only the fields that did.
    pub changed: Vec<MessageChange>,
    pub removed: Vec<MessageId>,
    /// Pass back on the next sync to receive only newer changes.
    pub delta_link: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    Text,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub content_type: BodyType,
    pub content: String,
}

/// A mail service. Graph is the only real implementation; tests use a fake.
pub trait MailBackend {
    fn list_folders(&self) -> impl Future<Output = Result<Vec<Folder>, BackendError>> + Send;

    fn delta_sync(
        &self,
        folder: &FolderId,
        delta_link: Option<&str>,
    ) -> impl Future<Output = Result<DeltaPage, BackendError>> + Send;

    fn fetch_body(
        &self,
        message: &MessageId,
    ) -> impl Future<Output = Result<Body, BackendError>> + Send;

    /// Deletes a message: to Deleted Items, or for good when `permanent`.
    fn delete(
        &self,
        message: &MessageId,
        permanent: bool,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Marks a message as read.
    fn mark_read(
        &self,
        message: &MessageId,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Sends a message, keeping a copy in Sent Items.
    fn send(&self, draft: &Draft) -> impl Future<Output = Result<(), BackendError>> + Send;
}
