use std::future::Future;

use jiff::Timestamp;

use crate::model::{
    Appointment, AppointmentDraft, Draft, Folder, FolderId, Message, MessageChange, MessageId,
};

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

/// A page of a folder's mail older than its sync reaches, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OlderPage {
    pub messages: Vec<Message>,
    /// Whether there is older mail still, past this page.
    pub more: bool,
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

/// An Outlook account's mail and calendar. Graph is the only real
/// implementation; tests use a fake.
pub trait MailBackend {
    fn list_folders(&self) -> impl Future<Output = Result<Vec<Folder>, BackendError>> + Send;

    fn delta_sync(
        &self,
        folder: &FolderId,
        delta_link: Option<&str>,
    ) -> impl Future<Output = Result<DeltaPage, BackendError>> + Send;

    /// One page of a folder's messages received at or before `before`,
    /// newest first, and only unread ones when `unread_only`. Delta sync
    /// reaches back only so far; this fetches past it on demand.
    fn older_messages(
        &self,
        folder: &FolderId,
        before: Timestamp,
        unread_only: bool,
    ) -> impl Future<Output = Result<OlderPage, BackendError>> + Send;

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

    /// Moves a message into another folder.
    fn move_message(
        &self,
        message: &MessageId,
        to: &FolderId,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Deletes a folder and everything in it: to Deleted Items, or for good
    /// when `permanent`.
    fn delete_folder(
        &self,
        folder: &FolderId,
        permanent: bool,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Makes a folder called `name` inside `parent`, or at the top level.
    fn create_folder(
        &self,
        parent: Option<&FolderId>,
        name: &str,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Gives a folder a new name.
    fn rename_folder(
        &self,
        folder: &FolderId,
        name: &str,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Marks a message as read.
    fn mark_read(
        &self,
        message: &MessageId,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// A folder's pinned messages, however old.
    fn pinned_messages(
        &self,
        folder: &FolderId,
    ) -> impl Future<Output = Result<Vec<Message>, BackendError>> + Send;

    /// Pins a message to the top of its folder, or unpins it.
    fn set_pinned(
        &self,
        message: &MessageId,
        pinned: bool,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Flags a message for follow-up, or clears its flag.
    fn set_flagged(
        &self,
        message: &MessageId,
        flagged: bool,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// Sends a message, keeping a copy in Sent Items.
    fn send(&self, draft: &Draft) -> impl Future<Output = Result<(), BackendError>> + Send;

    /// The appointments that overlap `from` to `to`, a recurring series
    /// once for each occurrence, soonest first.
    fn appointments(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<Appointment>, BackendError>> + Send;

    /// Puts a new appointment in the calendar, inviting its attendees.
    fn create_appointment(
        &self,
        draft: &AppointmentDraft,
    ) -> impl Future<Output = Result<(), BackendError>> + Send;
}
