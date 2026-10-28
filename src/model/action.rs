use crate::model::{FolderId, MessageId};

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
    /// Show the selected folder's subfolders, or go into them.
    Expand,
    /// Hide the selected folder's subfolders, or go up to its parent.
    Collapse,
    /// Add the selected folder to the favorites, or take it out.
    ToggleFavorite,
    Quit,
    /// Fill the screen with the focused pane, or show every pane again.
    ToggleZoom,
    /// Hide or show the folder pane.
    ToggleFolders,
    /// Mark or unmark the selected message, then move down.
    ToggleMark,
    MarkAll,
    ClearMarks,
    /// Move the marked or selected messages, or in the folder pane the
    /// selected folder, to Deleted Items, after asking.
    Delete,
    /// Delete the marked or selected messages, or the selected folder, for
    /// good, after asking.
    DeletePermanently,
    /// Sync every folder now.
    Refresh,
    /// Show the keys until the next key press.
    Help,
    /// Write a new message.
    Compose,
    /// Write the signature of the account being looked at.
    EditSignature,
    /// Answer the selected message's sender.
    Reply,
    /// Answer the sender and everyone else on the selected message.
    ReplyAll,
    Forward,
    /// Search every cached folder.
    Search,
}

/// Something core tells the UI happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The folder tree in the store changed.
    FoldersSynced,
    /// A folder's messages in the store changed.
    FolderSynced(FolderId),
    /// A sync of every folder is done, whether or not each folder made it.
    SyncFinished,
    BodyLoaded {
        message: MessageId,
        text: String,
    },
    /// A page of a folder's older mail is in the store; `more` says whether
    /// there is older mail still to fetch.
    OlderLoaded {
        folder: FolderId,
        unread_only: bool,
        more: bool,
    },
    /// The folder was deleted. The folder tree is synced right after.
    FolderDeleted(FolderId),
    /// These messages were deleted and are gone from the store.
    MessagesDeleted(Vec<MessageId>),
    Sent,
    /// The message being sent was not; it is still open to try again.
    SendFailed(String),
    /// The account is blocked from sending; holds what the service said.
    SendBlocked(String),
    Error(String),
}
