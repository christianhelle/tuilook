//! The folder tree: which folders are expanded, and so which are shown.

use std::collections::HashSet;

use crate::model::FolderId;

use crate::tui::app::{AppState, Effect};

impl AppState {
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

    /// → in the folder pane: expands the folder, or if it already is,
    /// moves to its first subfolder.
    pub(crate) fn expand_folder(&mut self) -> Option<Effect> {
        let index = self.selected_folder;
        if self.folders.is_empty() || !self.has_subfolders(index) {
            return None;
        }
        if self.expanded.insert(self.folders[index].id.clone()) {
            return None;
        }
        // The first subfolder comes straight after its parent.
        self.select_folder(index + 1)
    }

    /// ← in the folder pane: collapses the folder, or if it already is (or
    /// has no subfolders), moves to its parent.
    pub(crate) fn collapse_folder(&mut self) -> Option<Effect> {
        let folder = self.folders.get(self.selected_folder)?;
        if self.expanded.remove(&folder.id) {
            return None;
        }
        let parent = folder.parent_id.as_ref()?;
        let index = self.folders.iter().position(|f| &f.id == parent)?;
        self.select_folder(index)
    }

    /// Selects a folder and asks for its messages.
    pub(crate) fn select_folder(&mut self, index: usize) -> Option<Effect> {
        if index >= self.folders.len() {
            return None;
        }
        // Picking a folder leaves a search, even the folder already picked.
        let searched = self.search.take().is_some_and(|s| s.results.is_some());
        if index == self.selected_folder && !searched {
            return None;
        }
        self.selected_folder = index;
        self.load_shown()
    }

    /// Expands every folder above the selected one, so it is shown.
    pub(crate) fn reveal_selected_folder(&mut self) {
        let mut parent = self
            .folders
            .get(self.selected_folder)
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
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, Folder};

    use super::*;
    use crate::tui::app::Pane;

    fn folder(id: &str, parent: Option<&str>, unread: u32) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: parent.map(|p| FolderId(p.to_owned())),
            name: id.to_owned(),
            well_known: None,
            unread,
            total: 0,
        }
    }

    /// inbox > receipts > 2026, inbox > travel, then archive.
    fn tree() -> AppState {
        let mut app = AppState::default();
        app.set_folders(vec![
            folder("inbox", None, 1),
            folder("receipts", Some("inbox"), 2),
            folder("2026", Some("receipts"), 4),
            folder("travel", Some("inbox"), 0),
            folder("archive", None, 0),
        ]);
        app
    }

    /// The folders shown below the unread section.
    fn names(app: &AppState) -> Vec<&str> {
        app.visible_folders()
            .into_iter()
            .filter(|&i| i >= app.unread_count())
            .map(|i| app.folders[i].name.as_str())
            .collect()
    }

    /// The index in `folders` of the row with `id`.
    fn at(app: &AppState, id: &str) -> usize {
        app.folders.iter().position(|f| f.id.0 == id).unwrap()
    }

    fn selected(app: &AppState) -> &str {
        &app.folders[app.selected_folder].name
    }

    fn load(id: &str) -> Option<Effect> {
        Some(Effect::LoadFolder(FolderId(id.to_owned())))
    }

    #[test]
    fn folders_start_collapsed() {
        let app = tree();

        assert_eq!(names(&app), ["inbox", "archive"]);
    }

    #[test]
    fn right_expands_then_moves_into_the_subfolders() {
        let mut app = tree();

        assert_eq!(app.update(Action::Expand), None);
        assert_eq!(names(&app), ["inbox", "receipts", "travel", "archive"]);
        assert_eq!(app.update(Action::Expand), load("receipts"));
        assert_eq!(app.update(Action::Expand), None);
        assert_eq!(
            names(&app),
            ["inbox", "receipts", "2026", "travel", "archive"]
        );
    }

    #[test]
    fn left_collapses_then_jumps_to_the_parent() {
        let mut app = tree();
        app.update(Action::Expand);
        app.update(Action::Expand);
        app.update(Action::Expand);
        app.update(Action::Expand);
        assert_eq!(selected(&app), "2026");

        assert_eq!(app.update(Action::Collapse), load("receipts"));
        assert_eq!(app.update(Action::Collapse), None);
        assert_eq!(names(&app), ["inbox", "receipts", "travel", "archive"]);
        assert_eq!(app.update(Action::Collapse), load("inbox"));
        assert_eq!(app.update(Action::Collapse), None);
        assert_eq!(names(&app), ["inbox", "archive"]);
        assert_eq!(app.update(Action::Collapse), None, "nothing above the top");
    }

    #[test]
    fn moving_skips_folders_inside_collapsed_ones() {
        let mut app = tree();

        assert_eq!(app.update(Action::SelectNext), load("archive"));
        assert_eq!(app.update(Action::SelectPrev), load("inbox"));
    }

    #[test]
    fn a_folder_without_subfolders_does_not_expand() {
        let mut app = tree();
        app.update(Action::Bottom);

        assert_eq!(app.update(Action::Expand), None);
        assert!(app.expanded.is_empty());
    }

    #[test]
    fn arrows_do_nothing_outside_the_folder_pane() {
        let mut app = tree();
        app.focus = Pane::Messages;

        app.update(Action::Expand);

        assert!(app.expanded.is_empty());
    }

    #[test]
    fn a_collapsed_folder_counts_the_unread_mail_inside_it() {
        let mut app = tree();

        assert_eq!(app.unread_shown(at(&app, "inbox")), 7);
        app.update(Action::Expand);
        assert_eq!(app.unread_shown(at(&app, "inbox")), 1);
        assert_eq!(
            app.unread_shown(at(&app, "receipts")),
            6,
            "receipts is still collapsed"
        );
        assert_eq!(app.unread_shown(at(&app, "archive")), 0);
    }

    #[test]
    fn the_folders_above_the_selected_one_are_expanded_to_show_it() {
        let mut app = tree();
        app.selected_folder = at(&app, "2026");

        app.reveal_selected_folder();

        assert_eq!(
            names(&app),
            ["inbox", "receipts", "2026", "travel", "archive"]
        );
    }

    #[test]
    fn a_folder_whose_parent_is_missing_is_shown_at_the_top() {
        let mut app = AppState::default();
        app.set_folders(vec![folder("orphan", Some("gone"), 0)]);

        assert_eq!(names(&app), ["orphan"]);
    }

    /// me@outlook.com has an inbox with receipts in it, me@work.example an
    /// inbox. Graph lists top folders under a root tuilook does not keep.
    fn two_accounts() -> AppState {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
            ..Default::default()
        };
        app.set_account_folders(
            0,
            vec![
                folder("inbox", Some("root"), 1),
                folder("receipts", Some("inbox"), 2),
            ],
        );
        app.set_account_folders(1, vec![folder("work-inbox", Some("root2"), 4)]);
        app
    }

    #[test]
    fn each_account_heads_its_folders() {
        let app = two_accounts();

        assert_eq!(
            names(&app),
            ["me@outlook.com", "inbox", "me@work.example", "work-inbox"]
        );
        assert_eq!(app.folder_account[app.unread_count()..], [0, 0, 0, 1, 1]);
    }

    #[test]
    fn a_folded_account_counts_the_unread_mail_inside_it() {
        let mut app = two_accounts();
        app.selected_folder = at(&app, "work-inbox");

        assert_eq!(
            app.update(Action::Collapse),
            load("account:me@work.example")
        );
        app.update(Action::Collapse);

        assert_eq!(names(&app), ["me@outlook.com", "inbox", "me@work.example"]);
        assert_eq!(app.unread_shown(at(&app, "account:me@work.example")), 4);
    }

    #[test]
    fn a_folded_account_stays_folded_after_a_sync() {
        let mut app = two_accounts();
        app.selected_folder = at(&app, "account:me@outlook.com");
        app.update(Action::Collapse);

        app.set_account_folders(0, vec![folder("inbox", Some("root"), 1)]);

        assert_eq!(
            names(&app),
            ["me@outlook.com", "me@work.example", "work-inbox"]
        );
    }

    #[test]
    fn the_first_real_folder_is_selected_to_start() {
        let app = two_accounts();

        assert_eq!(selected(&app), "inbox");
    }

    #[test]
    fn a_single_account_has_no_row_of_its_own() {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned()],
            ..Default::default()
        };

        app.set_account_folders(0, vec![folder("inbox", Some("root"), 1)]);

        assert_eq!(names(&app), ["inbox"]);
    }

    #[test]
    fn effects_go_to_the_account_of_the_folder_looked_at() {
        let mut app = two_accounts();
        app.selected_folder = at(&app, "work-inbox");

        assert_eq!(app.account_for(&load("inbox").unwrap()), 0);
        assert_eq!(app.account_for(&load("work-inbox").unwrap()), 1);
        assert_eq!(
            app.account_for(&Effect::LoadBody(crate::model::MessageId("m".to_owned()))),
            1
        );
    }

    #[test]
    fn a_message_is_written_from_the_account_looked_at() {
        let mut app = two_accounts();
        app.selected_folder = at(&app, "work-inbox");

        app.update(Action::Compose);

        let compose = app.compose.as_ref().unwrap();
        assert_eq!(compose.account, 1);
        assert_eq!(compose.from.as_deref(), Some("me@work.example"));
    }

    #[test]
    fn a_sync_is_done_once_every_account_is() {
        let mut app = two_accounts();
        let now = std::time::Instant::now();

        app.start_sync();
        app.sync_finished(now);
        assert!(app.sync.running, "the other account is still syncing");
        app.sync_finished(now);
        assert!(!app.sync.running);
    }

    #[test]
    fn each_account_has_its_own_sending_pace() {
        let mut app = two_accounts();
        let now = std::time::Instant::now();
        app.selected_folder = at(&app, "work-inbox");
        app.update(Action::Compose);

        app.mail_sent(now);

        assert_eq!(app.sent_lately(1, now), 1);
        assert_eq!(app.sent_lately(0, now), 0);
    }
}
