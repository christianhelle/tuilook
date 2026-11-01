//! The folder pane: every account's folders, how they are laid out as rows,
//! and which row is selected.

use std::collections::{BTreeSet, HashSet};

use crate::model::{Folder, FolderId, WellKnown};
use crate::tui::rows::{
    Row, account_row, favorite_row, is_account_row, is_favorite_row, is_own_row, is_unread_row,
    row_target, unread_row,
};

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

    /// Takes what core last reported of `account`'s folders. An account
    /// called `address` starts with its folders shown, in both sections.
    pub fn report(&mut self, account: usize, folders: Vec<Folder>, address: Option<&str>) {
        if self.account_folders.len() <= account {
            self.account_folders.resize(account + 1, None);
        }
        let first = self.account_folders[account].replace(folders).is_none();
        if first && let Some(address) = address {
            self.expanded.insert(account_row(address));
            self.expanded.insert(unread_row(&account_row(address)));
        }
    }

    /// Lays out the rows again from the accounts' folders, the unread section
    /// and the favorites. The selected row stays, or the folder it stood for,
    /// or else the first real folder, so there is mail to look at. Everything
    /// above it is expanded, so it is shown.
    pub fn lay_out(&mut self, accounts: &[String]) {
        let previous = self.current_folder().cloned();
        let (folders, owners) = self.folder_tree(accounts);
        let find = |id: &FolderId| folders.iter().position(|f| &f.id == id);
        self.selected = previous
            .as_ref()
            .and_then(|id| find(id).or_else(|| find(&row_target(id))))
            .or_else(|| folders.iter().position(is_own_row))
            .unwrap_or(0);
        self.folders = folders;
        self.owners = owners;
        self.reveal_selected_folder();
    }

    /// The favorites' rows, with the account each is in. A favorite whose
    /// folder is not there, such as before its account has synced, is left
    /// out but kept. A name favorited from several accounts says which
    /// account each is from.
    fn favorite_rows(&self, accounts: &[String]) -> (Vec<Folder>, Vec<usize>) {
        let found: Vec<(usize, &Folder)> = self
            .favorites
            .iter()
            .filter_map(|id| {
                self.account_folders
                    .iter()
                    .enumerate()
                    .find_map(|(account, folders)| {
                        let folder = folders.as_deref()?.iter().find(|f| &f.id == id)?;
                        Some((account, folder))
                    })
            })
            .collect();
        let clashes = |account: usize, name: &str| {
            found
                .iter()
                .any(|&(other, f)| other != account && f.name.eq_ignore_ascii_case(name))
        };
        found
            .iter()
            .map(|&(account, folder)| {
                let name = match accounts.get(account) {
                    Some(address) if clashes(account, &folder.name) => {
                        format!("{} - {address}", folder.name)
                    }
                    _ => folder.name.clone(),
                };
                let row = Folder {
                    id: favorite_row(&folder.id),
                    parent_id: None,
                    name,
                    ..folder.clone()
                };
                (row, account)
            })
            .unzip()
    }

    /// The unread section's rows, with the account each is in. A folder is
    /// there while it, or a folder inside it, has unread mail; junk and
    /// deleted mail, and folders inside those, are left out. The selected
    /// row stays until it is left, so reading the last unread message does
    /// not pull the selection away.
    fn unread_rows(&self, accounts: &[String]) -> (Vec<Folder>, Vec<usize>) {
        let several = accounts.len() > 1;
        let kept = self
            .folders
            .get(self.selected)
            .filter(|f| is_unread_row(f))
            .map(|f| row_target(&f.id));
        let mut rows = Vec::new();
        let mut owners = Vec::new();
        for (account, folders) in self.account_folders.iter().enumerate() {
            let folders = folders.as_deref().unwrap_or_default();
            let ids: HashSet<&FolderId> = folders.iter().map(|f| &f.id).collect();
            // A parent comes before its subfolders, so one pass finds them all.
            let mut skipped: HashSet<&FolderId> = HashSet::new();
            for folder in folders {
                let junk = matches!(
                    folder.well_known,
                    Some(WellKnown::JunkEmail | WellKnown::DeletedItems)
                );
                if junk
                    || folder
                        .parent_id
                        .as_ref()
                        .is_some_and(|p| skipped.contains(p))
                {
                    skipped.insert(&folder.id);
                }
            }
            // Each folder to show, and every folder above it.
            let mut wanted: HashSet<&FolderId> = HashSet::new();
            for folder in folders {
                let kept = kept.as_ref() == Some(&folder.id);
                if !kept && (folder.unread == 0 || skipped.contains(&folder.id)) {
                    continue;
                }
                let mut at = Some(folder);
                while let Some(folder) = at.filter(|f| wanted.insert(&f.id)) {
                    at = folder
                        .parent_id
                        .as_ref()
                        .and_then(|p| folders.iter().find(|f| &f.id == p));
                }
            }
            let address = accounts.get(account).filter(|_| several);
            let heading = address.map(|address| account_row(address));
            let heading_kept = heading.is_some() && kept == heading;
            if wanted.is_empty() && !heading_kept {
                continue;
            }
            let top = match (&heading, address) {
                (Some(heading), Some(address)) => {
                    rows.push(Folder {
                        id: unread_row(heading),
                        parent_id: None,
                        name: address.clone(),
                        well_known: None,
                        unread: 0,
                        total: 0,
                    });
                    Some(unread_row(heading))
                }
                _ => None,
            };
            rows.extend(
                folders
                    .iter()
                    .filter(|f| wanted.contains(&f.id))
                    .map(|folder| {
                        let parent = match &folder.parent_id {
                            Some(p) if ids.contains(p) => Some(unread_row(p)),
                            _ => top.clone(),
                        };
                        Folder {
                            id: unread_row(&folder.id),
                            parent_id: parent,
                            ..folder.clone()
                        }
                    }),
            );
            owners.resize(rows.len(), account);
        }
        (rows, owners)
    }

    /// Every account's folders as one tree, with the account each is in.
    /// With several accounts, each heads a row of its own above its folders.
    /// The favorites come first, then the unread section.
    fn folder_tree(&self, accounts: &[String]) -> (Vec<Folder>, Vec<usize>) {
        let several = accounts.len() > 1;
        let (mut tree, mut owners) = self.favorite_rows(accounts);
        let (unread, unread_owners) = self.unread_rows(accounts);
        tree.extend(unread);
        owners.extend(unread_owners);
        for (account, folders) in self.account_folders.iter().enumerate() {
            let folders = folders.as_deref().unwrap_or_default();
            let Some(address) = accounts.get(account).filter(|_| several) else {
                tree.extend_from_slice(folders);
                owners.resize(tree.len(), account);
                continue;
            };
            let row = account_row(address);
            tree.push(Folder {
                id: row.clone(),
                parent_id: None,
                name: address.clone(),
                well_known: None,
                unread: 0,
                total: 0,
            });
            // The account's top folders are those whose parent it does not list.
            let ids: HashSet<&FolderId> = folders.iter().map(|f| &f.id).collect();
            tree.extend(folders.iter().map(|folder| {
                let mut folder = folder.clone();
                if !folder.parent_id.as_ref().is_some_and(|p| ids.contains(p)) {
                    folder.parent_id = Some(row.clone());
                }
                folder
            }));
            owners.resize(tree.len(), account);
        }
        (tree, owners)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fake;

    fn accounts() -> Vec<String> {
        vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()]
    }

    /// Two accounts' folders laid out, the first holding unread mail.
    fn pane() -> FolderPane {
        let mut pane = FolderPane::default();
        let inbox = Folder {
            unread: 2,
            ..fake::folder("inbox")
        };
        pane.report(0, vec![inbox], Some("me@outlook.com"));
        pane.report(1, vec![fake::folder("work")], Some("me@work.example"));
        pane.lay_out(&accounts());
        pane
    }

    fn names(pane: &FolderPane) -> Vec<&str> {
        pane.folders.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn the_unread_section_comes_before_each_account_heading_its_folders() {
        assert_eq!(
            names(&pane()),
            [
                "me@outlook.com",
                "inbox",
                "me@outlook.com",
                "inbox",
                "me@work.example",
                "work",
            ]
        );
    }

    #[test]
    fn each_row_knows_its_account() {
        let pane = pane();

        assert_eq!(pane.owners, [0, 0, 0, 0, 1, 1]);
        assert_eq!(pane.folder_home(&FolderId("work".to_owned())), 1);
    }

    #[test]
    fn the_first_real_folder_is_selected_to_begin_with() {
        let pane = pane();

        assert_eq!(pane.current_folder(), Some(&FolderId("inbox".to_owned())));
        assert!(!pane.shows_unread());
    }

    #[test]
    fn a_favorite_comes_first_and_stands_for_its_folder() {
        let mut pane = pane();
        pane.favorites = vec![FolderId("work".to_owned())];

        pane.lay_out(&accounts());

        assert_eq!(pane.favorite_count(), 1);
        let Some(Row::Folder(folder)) = pane.folder_row(0) else {
            panic!("the favorite shows a folder");
        };
        assert_eq!(folder.id, FolderId("work".to_owned()));
    }

    #[test]
    fn the_selection_stays_on_its_folder_when_the_rows_move() {
        let mut pane = pane();
        pane.selected = pane
            .folders
            .iter()
            .position(|f| f.id == FolderId("work".to_owned()))
            .unwrap();

        pane.favorites = vec![FolderId("inbox".to_owned())];
        pane.lay_out(&accounts());

        assert_eq!(pane.current_folder(), Some(&FolderId("work".to_owned())));
    }

    #[test]
    fn a_folder_that_went_away_leaves_the_selection_on_the_first_real_folder() {
        let mut pane = pane();
        pane.selected = 5;

        pane.report(1, vec![], Some("me@work.example"));
        pane.lay_out(&accounts());

        assert_eq!(pane.current_folder(), Some(&FolderId("inbox".to_owned())));
    }
}
