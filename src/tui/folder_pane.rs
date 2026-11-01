//! The folder pane: every account's folders, how they are laid out as rows,
//! and which row is selected.

use std::collections::{BTreeSet, HashSet};

use crate::model::{Folder, FolderId};
use crate::tui::rows::{Row, is_account_row, is_favorite_row, is_unread_row, row_target};

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

impl FolderPane {
    /// Indexes into `folders` of the folders shown, in order: the top-level
    /// ones, and the subfolders of expanded folders that are shown.
    pub fn visible_folders(&self) -> Vec<usize> {
        let mut earlier: HashSet<&FolderId> = HashSet::new();
        let mut shown: HashSet<&FolderId> = HashSet::new();
        let mut visible = Vec::new();
        for (index, folder) in self.folders.iter().enumerate() {
            let show = match &folder.parent_id {
                // The tree lists a parent before its subfolders; a folder
                // whose parent is not before it is shown at the top level.
                Some(parent) if earlier.contains(parent) => {
                    shown.contains(parent) && self.expanded.contains(parent)
                }
                _ => true,
            };
            earlier.insert(&folder.id);
            if show {
                shown.insert(&folder.id);
                visible.push(index);
            }
        }
        visible
    }

    pub fn has_subfolders(&self, index: usize) -> bool {
        let id = &self.folders[index].id;
        self.folders
            .iter()
            .any(|f| f.parent_id.as_ref() == Some(id))
    }

    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.contains(&self.folders[index].id)
    }

    /// Unread messages in a folder, counting its subfolders' while they are
    /// folded away inside it so new mail is never hidden.
    pub fn unread_shown(&self, index: usize) -> u32 {
        if self.is_expanded(index) {
            return self.folders[index].unread;
        }
        let mut inside = HashSet::from([&self.folders[index].id]);
        let mut unread = self.folders[index].unread;
        // Subfolders follow their parent, so one pass finds them all.
        for folder in &self.folders[index + 1..] {
            match &folder.parent_id {
                Some(parent) if inside.contains(parent) => {
                    inside.insert(&folder.id);
                    unread += folder.unread;
                }
                _ => break,
            }
        }
        unread
    }

    /// Expands every folder above the selected one, so it is shown.
    pub fn reveal_selected_folder(&mut self) {
        let mut parent = self
            .folders
            .get(self.selected)
            .and_then(|f| f.parent_id.clone());
        while let Some(id) = parent {
            let Some(folder) = self.folders.iter().find(|f| f.id == id) else {
                break;
            };
            // Above an expanded folder may still be a folded account.
            self.expanded.insert(id);
            parent = folder.parent_id.clone();
        }
    }

    /// How many rows the favorites take at the top of `folders`.
    pub fn favorite_count(&self) -> usize {
        self.folders
            .iter()
            .take_while(|f| is_favorite_row(f))
            .count()
    }

    /// How many rows of the unread section are shown, its folded folders'
    /// subfolders left out.
    pub fn unread_rows_shown(&self) -> usize {
        self.visible_folders()
            .into_iter()
            .filter(|&i| is_unread_row(&self.folders[i]))
            .count()
    }

    /// How many rows the unread section takes, below the favorites.
    pub fn unread_count(&self) -> usize {
        self.folders
            .iter()
            .skip(self.favorite_count())
            .take_while(|f| is_unread_row(f))
            .count()
    }

    /// Whether the selected row is in the unread section, and so the
    /// message list shows only unread mail.
    pub fn shows_unread(&self) -> bool {
        self.folders.get(self.selected).is_some_and(is_unread_row)
    }

    /// What the row at `index` of `folders` stands for.
    pub fn folder_row(&self, index: usize) -> Option<Row<'_>> {
        let row = self.folders.get(index)?;
        if is_account_row(row) {
            return Some(Row::Account);
        }
        self.real_folder(&row_target(&row.id)).map(Row::Folder)
    }

    /// What the selected row of the folder pane stands for.
    pub fn selected_folder_row(&self) -> Option<Row<'_>> {
        self.folder_row(self.selected)
    }

    /// The folder `id` itself, not a favorite or unread row standing for it.
    pub fn real_folder(&self, id: &FolderId) -> Option<&Folder> {
        self.folders.iter().find(|f| &f.id == id)
    }

    /// The account the folder at `index` belongs to.
    pub fn account_of(&self, index: usize) -> usize {
        self.owners.get(index).copied().unwrap_or(0)
    }

    /// The account folder `id` is in.
    pub fn folder_home(&self, id: &FolderId) -> usize {
        self.folders
            .iter()
            .position(|f| &f.id == id)
            .map_or(0, |index| self.account_of(index))
    }

    /// The id of the selected row, which for a favorite or unread row is not
    /// the folder's.
    pub fn current_folder(&self) -> Option<&FolderId> {
        self.folders.get(self.selected).map(|f| &f.id)
    }
}
