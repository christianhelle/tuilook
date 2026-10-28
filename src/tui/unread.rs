//! The unread section: the accounts' folders again, but only those with
//! unread mail, each showing only its unread messages.

use std::collections::HashSet;

use crate::model::{Folder, FolderId, MessageId, WellKnown};

use crate::tui::app::{AppState, account_row, is_unread_row, row_target, unread_row};

impl AppState {
    /// The unread section's rows, with the account each is in. A folder is
    /// there while it, or a folder inside it, has unread mail; junk and
    /// deleted mail, and folders inside those, are left out. The selected
    /// row stays until it is left, so reading the last unread message does
    /// not pull the selection away.
    pub(crate) fn unread_rows(&self) -> (Vec<Folder>, Vec<usize>) {
        let several = self.accounts.len() > 1;
        let kept = self
            .folders
            .get(self.selected_folder)
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
            let address = self.accounts.get(account).filter(|_| several);
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

    /// How many rows the unread section takes, below the favorites.
    pub fn unread_count(&self) -> usize {
        self.folders
            .iter()
            .skip(self.favorite_count())
            .take_while(|f| is_unread_row(f))
            .count()
    }

    /// The read messages the unread list keeps when it is loaded again after
    /// a sync: every one it shows, so going back to mail just read still
    /// works. After F5 only the one being read stays. Picking the folder
    /// again drops them all.
    pub fn kept_read(&self) -> Vec<MessageId> {
        if self.sync.forced {
            let selected = self.selected();
            return selected.map(|m| m.id.clone()).into_iter().collect();
        }
        self.messages.iter().map(|m| m.id.clone()).collect()
    }

    /// Whether the selected row is in the unread section, and so the
    /// message list shows only unread mail.
    pub fn shows_unread(&self) -> bool {
        self.folders
            .get(self.selected_folder)
            .is_some_and(is_unread_row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Effect;

    fn folder(id: &str, name: &str, parent: Option<&str>, unread: u32) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: parent.map(|p| FolderId(p.to_owned())),
            name: name.to_owned(),
            well_known: None,
            unread,
            total: 0,
        }
    }

    /// me@outlook.com has a read Inbox with an unread Receipts in it, and a
    /// read Archive; me@work.example an unread Inbox and a read Projects.
    fn outlook(receipts: u32) -> Vec<Folder> {
        vec![
            folder("inbox", "Inbox", Some("root"), 0),
            folder("receipts", "Receipts", Some("inbox"), receipts),
            folder("archive", "Archive", Some("root"), 0),
        ]
    }

    fn two_accounts() -> AppState {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
            ..Default::default()
        };
        app.set_account_folders(0, outlook(2));
        app.set_account_folders(
            1,
            vec![
                folder("work-inbox", "Inbox", Some("root2"), 4),
                folder("projects", "Projects", Some("root2"), 0),
            ],
        );
        app
    }

    fn unread_names(app: &AppState) -> Vec<&str> {
        let rows = app.unread_count();
        app.visible_folders()
            .into_iter()
            .filter(|&i| i < rows)
            .map(|i| app.folders[i].name.as_str())
            .collect()
    }

    fn select(app: &mut AppState, id: &str) -> Option<Effect> {
        let index = app.folders.iter().position(|f| f.id.0 == id).unwrap();
        app.select_folder(index)
    }

    #[test]
    fn only_folders_with_unread_mail_are_listed_under_their_account() {
        let app = two_accounts();

        assert_eq!(
            unread_names(&app),
            ["me@outlook.com", "Inbox", "me@work.example", "Inbox"],
            "Receipts is inside the collapsed Inbox"
        );
        assert_eq!(app.unread_count(), 5);
        assert_eq!(app.unread_shown(1), 2, "Inbox counts Receipts' unread mail");
    }

    #[test]
    fn the_first_real_folder_is_still_selected_to_start() {
        let app = two_accounts();

        assert_eq!(app.current_folder(), Some(&FolderId("inbox".to_owned())));
        assert!(!app.shows_unread());
    }

    #[test]
    fn picking_one_shows_its_unread_mail_from_its_account() {
        let mut app = two_accounts();

        let effect = select(&mut app, "unread:work-inbox");

        let load = Effect::LoadUnread(FolderId("work-inbox".to_owned()));
        assert_eq!(effect, Some(load.clone()));
        assert!(app.shows_unread());
        assert_eq!(app.account_for(&load), 1);
        assert_eq!(app.current_account(), 1);
    }

    #[test]
    fn a_folder_read_while_selected_stays_until_it_is_left() {
        let mut app = two_accounts();
        select(&mut app, "unread:receipts");

        assert_eq!(app.set_account_folders(0, outlook(0)), None);
        assert_eq!(
            app.current_folder(),
            Some(&FolderId("unread:receipts".to_owned()))
        );

        select(&mut app, "unread:work-inbox");
        app.set_account_folders(0, outlook(0));
        assert_eq!(unread_names(&app), ["me@work.example", "Inbox"]);
    }

    #[test]
    fn junk_and_deleted_mail_are_left_out() {
        let mut app = AppState::default();
        let mut junk = folder("junk", "Junk Email", Some("root"), 3);
        junk.well_known = Some(WellKnown::JunkEmail);
        let mut deleted = folder("deleted", "Deleted Items", Some("root"), 0);
        deleted.well_known = Some(WellKnown::DeletedItems);

        app.set_folders(vec![
            folder("inbox", "Inbox", Some("root"), 1),
            junk,
            deleted,
            folder("old", "Old", Some("deleted"), 2),
        ]);

        assert_eq!(unread_names(&app), ["Inbox"]);
    }

    fn message(id: &str, is_read: bool) -> crate::model::Message {
        crate::model::Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("work-inbox".to_owned()),
            conversation_id: String::new(),
            subject: id.to_owned(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: jiff::Timestamp::UNIX_EPOCH,
            is_read,
            flagged: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    fn ids(ids: &[&str]) -> Vec<MessageId> {
        ids.iter().map(|id| MessageId((*id).to_owned())).collect()
    }

    #[test]
    fn mail_read_in_the_list_stays_until_f5() {
        let mut app = two_accounts();
        select(&mut app, "unread:work-inbox");
        app.set_messages(vec![message("m1", true), message("m2", false)]);
        app.selected_message = 1;

        assert_eq!(app.kept_read(), ids(&["m1", "m2"]));

        app.update(crate::model::Action::Refresh);
        assert_eq!(app.kept_read(), ids(&["m2"]), "only the one being read");

        app.sync_finished(std::time::Instant::now());
        app.sync_finished(std::time::Instant::now());
        assert_eq!(app.kept_read(), ids(&["m1", "m2"]));
    }

    #[test]
    fn a_single_account_has_no_row_of_its_own() {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned()],
            ..Default::default()
        };

        app.set_account_folders(0, outlook(1));

        assert_eq!(unread_names(&app), ["Inbox"]);
    }

    #[test]
    fn nothing_unread_leaves_the_section_empty() {
        let mut app = AppState::default();

        app.set_folders(outlook(0));

        assert_eq!(app.unread_count(), 0);
    }
}
