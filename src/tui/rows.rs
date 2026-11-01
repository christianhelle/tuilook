//! The rows of the folder pane. Besides each account's folders, the pane
//! has rows heading each account, and the favorites and unread section
//! showing folders again. Those rows are folders too, so the tree's
//! expanding and nesting work for every section, with ids that say what
//! they stand for.

use crate::model::{Folder, FolderId};

use crate::tui::app::AppState;

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

/// Whether `folder` is an account's folder in that account's tree, not a
/// row heading it or showing a folder again.
pub fn is_own_row(folder: &Folder) -> bool {
    !is_account_row(folder) && !is_favorite_row(folder) && !is_unread_row(folder)
}

/// What a row of the folder pane stands for. Ask the app for it, rather than
/// reading it from the row's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row<'a> {
    /// The row heading an account's folders, here or in the unread section.
    Account,
    /// An account's folder, whether its own row, a favorite or an unread
    /// row shows it.
    Folder(&'a Folder),
}

impl AppState {}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, unread: u32) -> Folder {
        Folder {
            unread,
            total: unread,
            ..crate::core::fake::folder(id)
        }
    }

    #[test]
    fn favorite_and_unread_rows_stand_for_the_folder_itself() {
        let mut app = AppState::default();
        app.folder_pane.favorites = vec![FolderId("news".to_owned())];
        app.set_folders(vec![folder("news", 2), folder("bills", 0)]);
        let news = app
            .folder_pane
            .real_folder(&FolderId("news".to_owned()))
            .unwrap();

        let rows: Vec<_> = (0..app.folder_pane.folders.len())
            .filter_map(|i| app.folder_pane.folder_row(i))
            .filter(|row| *row == Row::Folder(news))
            .collect();

        assert_eq!(rows.len(), 3, "its own row, a favorite and an unread row");
    }
}
