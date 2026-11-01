//! The folder pane: every account's folders, how they are laid out as rows,
//! and which row is selected.

use std::collections::BTreeSet;

use crate::model::{Folder, FolderId};

/// What the folder pane holds. Its rows are folders, each account's own and
/// those standing for them; see [`crate::tui::rows`].
#[derive(Debug, Default)]
pub struct FolderPane {
    /// The rows, in order.
    pub folders: Vec<Folder>,
    /// The row selected.
    pub selected: usize,
    /// The account each row belongs to.
    pub owners: Vec<usize>,
    /// Each account's folders as core last reported them, or `None` before
    /// the first report.
    pub account_folders: Vec<Option<Vec<Folder>>>,
    /// Folders whose subfolders are shown.
    pub expanded: BTreeSet<FolderId>,
    /// Folders shown again in the favorites above the accounts, in the
    /// order they were added.
    pub favorites: Vec<FolderId>,
}
