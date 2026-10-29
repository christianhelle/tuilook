//! The rows of the folder pane. Besides each account's folders, the pane
//! has rows heading each account, and the favorites and unread section
//! showing folders again. Those rows are folders too, so the tree's
//! expanding and nesting work for every section, with ids that say what
//! they stand for.

use crate::model::{Folder, FolderId};

/// Folder ids of the rows heading each account start with this.
const ACCOUNT_ROW: &str = "account:";

/// Folder ids of the rows in the favorites start with this, followed by
/// the id of the folder they show.
const FAVORITE_ROW: &str = "favorite:";

/// Folder ids of the rows in the unread section start with this, followed
/// by the id of the folder or account row they stand for.
const UNREAD_ROW: &str = "unread:";

/// The id of the favorites row showing folder `id`.
pub fn favorite_row(id: &FolderId) -> FolderId {
    FolderId(format!("{FAVORITE_ROW}{}", id.0))
}

/// Whether `folder` is a row in the favorites.
pub fn is_favorite_row(folder: &Folder) -> bool {
    folder.id.0.starts_with(FAVORITE_ROW)
}

/// The id of the unread section's row standing for `id`.
pub fn unread_row(id: &FolderId) -> FolderId {
    FolderId(format!("{UNREAD_ROW}{}", id.0))
}

/// Whether `folder` is a row in the unread section.
pub fn is_unread_row(folder: &Folder) -> bool {
    folder.id.0.starts_with(UNREAD_ROW)
}

/// The folder whose messages a row shows: the folder itself, or the one a
/// favorite or unread row stands for.
pub fn row_target(row: &FolderId) -> FolderId {
    let id = &row.0;
    match id
        .strip_prefix(FAVORITE_ROW)
        .or_else(|| id.strip_prefix(UNREAD_ROW))
    {
        Some(id) => FolderId(id.to_owned()),
        None => row.clone(),
    }
}

/// The id of the row heading `address`'s folders.
pub fn account_row(address: &str) -> FolderId {
    FolderId(format!("{ACCOUNT_ROW}{address}"))
}

/// Whether `folder` is the row heading an account, not a real folder,
/// here or in the unread section.
pub fn is_account_row(folder: &Folder) -> bool {
    row_target(&folder.id).0.starts_with(ACCOUNT_ROW)
}
