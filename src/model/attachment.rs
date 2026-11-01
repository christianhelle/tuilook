/// Graph attachment id, unique within its message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttachmentId(pub String);

/// What is attached, which decides how it can be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    /// A file, saved as it is.
    File,
    /// Another mail or calendar item, saved as an `.eml` file.
    Item,
    /// A link to a file in the cloud, which has nothing to download.
    Reference,
}

/// A file attached to a message. Pictures shown in the body are left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    pub name: String,
    pub content_type: String,
    /// In bytes.
    pub size: u64,
    pub kind: AttachmentKind,
}
