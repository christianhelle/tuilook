use crate::{FolderId, MessageId};

/// Something the user asked for. The UI applies it to its own state and
/// forwards anything that needs IO to core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    FocusNextPane,
    FocusPrevPane,
    SelectNext,
    SelectPrev,
    PageDown,
    PageUp,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
    Open,
    Quit,
}

/// Something core tells the UI happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The folder tree in the store changed.
    FoldersSynced,
    /// A folder's messages in the store changed.
    FolderSynced(FolderId),
    BodyLoaded {
        message: MessageId,
        text: String,
    },
    Error(String),
}
