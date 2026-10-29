//! Favorites: folders picked to be shown again above the accounts.

use crate::model::Folder;

use crate::tui::{
    app::{AppState, Effect},
    rows::{favorite_row, is_account_row, is_favorite_row, row_target},
};

impl AppState {
    /// The favorites' rows, with the account each is in. A favorite whose
    /// folder is not there, such as before its account has synced, is left
    /// out but kept. A name favorited from several accounts says which
    /// account each is from.
    pub(crate) fn favorite_rows(&self) -> (Vec<Folder>, Vec<usize>) {
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
                let name = match self.accounts.get(account) {
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

    /// How many rows the favorites take at the top of `folders`.
    pub fn favorite_count(&self) -> usize {
        self.folders
            .iter()
            .take_while(|f| is_favorite_row(f))
            .count()
    }

    /// `*` in the folder pane: adds the selected folder to the favorites,
    /// or takes it out if it is in them already.
    pub(crate) fn toggle_favorite(&mut self) -> Option<Effect> {
        let row = self.folders.get(self.selected_folder)?;
        if is_account_row(row) {
            return None;
        }
        let id = row_target(&row.id);
        // A favorite's own name may say which account it is from.
        let name = self
            .folders
            .iter()
            .find(|f| f.id == id)
            .map_or_else(|| row.name.clone(), |f| f.name.clone());
        match self.favorites.iter().position(|f| f == &id) {
            Some(index) => {
                self.favorites.remove(index);
                self.show_info(format!("Removed {name} from Favorites"));
            }
            None => {
                self.favorites.push(id);
                self.show_info(format!("Added {name} to Favorites"));
            }
        }
        self.rebuild_folders()
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Action, FolderId};

    use super::*;
    use crate::tui::app::Pane;

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

    /// me@outlook.com has an Inbox with Receipts in it, me@work.example an
    /// Inbox and Projects.
    fn two_accounts() -> AppState {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
            ..Default::default()
        };
        app.set_account_folders(
            0,
            vec![
                folder("inbox", "Inbox", Some("root"), 1),
                folder("receipts", "Receipts", Some("inbox"), 2),
            ],
        );
        app.set_account_folders(
            1,
            vec![
                folder("work-inbox", "Inbox", Some("root2"), 4),
                folder("projects", "Projects", Some("root2"), 0),
            ],
        );
        app
    }

    fn names(app: &AppState) -> Vec<&str> {
        app.visible_folders()
            .into_iter()
            .map(|i| app.folders[i].name.as_str())
            .collect()
    }

    fn select(app: &mut AppState, id: &str) {
        app.selected_folder = app.folders.iter().position(|f| f.id.0 == id).unwrap();
    }

    fn load(id: &str) -> Option<Effect> {
        Some(Effect::LoadFolder(FolderId(id.to_owned())))
    }

    #[test]
    fn star_adds_the_folder_to_the_favorites_at_the_top() {
        let mut app = two_accounts();
        select(&mut app, "receipts");

        assert_eq!(app.update(Action::ToggleFavorite), None);

        assert_eq!(app.favorite_count(), 1);
        assert_eq!(names(&app)[0], "Receipts");
        assert_eq!(app.current_folder(), Some(&FolderId("receipts".to_owned())));
    }

    #[test]
    fn star_again_takes_it_out_from_either_row() {
        let mut app = two_accounts();
        select(&mut app, "projects");
        app.update(Action::ToggleFavorite);
        app.update(Action::ToggleFavorite);
        assert_eq!(app.favorite_count(), 0);

        app.update(Action::ToggleFavorite);
        select(&mut app, "favorite:projects");
        app.update(Action::ToggleFavorite);

        assert_eq!(app.favorite_count(), 0);
        assert_eq!(
            app.current_folder(),
            Some(&FolderId("projects".to_owned())),
            "the folder stays selected in its account"
        );
    }

    #[test]
    fn star_does_nothing_on_an_account_or_outside_the_folder_pane() {
        let mut app = two_accounts();
        select(&mut app, "account:me@outlook.com");
        app.update(Action::ToggleFavorite);
        select(&mut app, "inbox");
        app.focus = Pane::Messages;
        app.update(Action::ToggleFavorite);

        assert!(app.favorites.is_empty());
    }

    #[test]
    fn inboxes_from_several_accounts_say_which_account_they_are_from() {
        let mut app = two_accounts();
        for id in ["inbox", "work-inbox", "projects"] {
            select(&mut app, id);
            app.update(Action::ToggleFavorite);
        }

        assert_eq!(
            names(&app)[..3],
            [
                "Inbox - me@outlook.com",
                "Inbox - me@work.example",
                "Projects"
            ]
        );
    }

    #[test]
    fn a_favorite_shows_its_folder_from_its_account() {
        let mut app = two_accounts();
        select(&mut app, "work-inbox");
        app.update(Action::ToggleFavorite);
        select(&mut app, "inbox");

        assert_eq!(app.update(Action::Top), load("work-inbox"));
        assert_eq!(app.shown_folder(), Some(FolderId("work-inbox".to_owned())));
        assert_eq!(app.account_for(&load("work-inbox").unwrap()), 1);
        assert_eq!(app.current_account(), 1);
        assert_eq!(app.unread_shown(0), 4);
    }

    #[test]
    fn moving_down_goes_from_the_favorites_into_the_unread_section() {
        let mut app = two_accounts();
        select(&mut app, "projects");
        app.update(Action::ToggleFavorite);
        app.update(Action::Top);

        assert_eq!(
            app.update(Action::SelectNext),
            Some(Effect::LoadUnread(FolderId(
                "account:me@outlook.com".to_owned()
            )))
        );
    }

    #[test]
    fn a_selected_favorite_stays_selected_after_a_sync() {
        let mut app = two_accounts();
        select(&mut app, "receipts");
        app.update(Action::ToggleFavorite);
        app.update(Action::Top);

        let effect = app.set_account_folders(
            0,
            vec![
                folder("inbox", "Inbox", Some("root"), 0),
                folder("receipts", "Receipts", Some("inbox"), 5),
            ],
        );

        assert_eq!(effect, None);
        assert_eq!(
            app.current_folder(),
            Some(&FolderId("favorite:receipts".to_owned()))
        );
        assert_eq!(app.folders[0].unread, 5);
    }

    #[test]
    fn a_favorite_whose_folder_is_missing_waits_for_it() {
        let mut app = AppState {
            favorites: vec![FolderId("later".to_owned())],
            ..Default::default()
        };
        app.set_folders(vec![folder("inbox", "Inbox", None, 0)]);
        assert_eq!(app.favorite_count(), 0);

        app.set_folders(vec![
            folder("inbox", "Inbox", None, 0),
            folder("later", "Later", None, 0),
        ]);

        assert_eq!(names(&app), ["Later", "Inbox", "Later"]);
    }
}
