//! Managing the signed-in accounts: adding one, signing one in again or
//! with another tenant or app, writing its signature, moving it up or down
//! and removing it.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::tui::{
    app::{AppState, Confirm, Effect},
    dialog::Dialog,
    input::TextInput,
};

/// What the UI asks of whatever signs accounts in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountRequest {
    /// Sign in with `login`, as the account at `replacing` if given.
    SignIn {
        login: Login,
        replacing: Option<String>,
    },
    /// Stop the sign-in under way.
    Cancel,
}

/// How to start again once the UI closes for accounts to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reload {
    /// Show the accounts as they are saved now.
    Keep,
    /// Forget the account at this address first, once its cache is closed.
    Remove(String),
}

/// What an account signs in with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Login {
    pub tenant: String,
    pub client_id: String,
}

/// What a key asks for while managing accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Sign in with `login`, as the account at `replacing` if given.
    SignIn {
        login: Login,
        replacing: Option<String>,
    },
    /// Stop waiting for the sign-in under way.
    Cancel,
    /// Remove the account at this address, after asking.
    Remove(String),
    /// Move the account at `address` `delta` places later.
    Move {
        address: String,
        delta: isize,
    },
    /// Write the signature of the account at this address.
    Signature(String),
}

/// Which field of the form the keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Tenant,
    ClientId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// The accounts, one picked.
    List,
    /// What to sign in with, for a new account or the one at `editing`.
    Form {
        editing: Option<String>,
        tenant: TextInput,
        client_id: TextInput,
        field: Field,
    },
    /// Waiting for the user to sign in, with what Microsoft says to do.
    Waiting(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountManager {
    /// Each account's address and what it signs in with, in order.
    pub accounts: Vec<(String, Login)>,
    pub selected: usize,
    /// What a new account signs in with unless changed.
    pub defaults: Login,
    pub mode: Mode,
}

impl AccountManager {
    pub fn new(accounts: Vec<(String, Login)>, selected: usize, defaults: Login) -> Self {
        let selected = selected.min(accounts.len().saturating_sub(1));
        Self {
            accounts,
            selected,
            defaults,
            mode: Mode::List,
        }
    }

    /// Whether text is typed into the dialog.
    pub fn takes_text(&self) -> bool {
        matches!(self.mode, Mode::Form { .. })
    }

    /// The picked account's address and login, if there are any.
    pub fn picked(&self) -> Option<&(String, Login)> {
        self.accounts.get(self.selected)
    }

    /// Shows what Microsoft says to do to sign in.
    pub fn show_code(&mut self, message: String) {
        if matches!(self.mode, Mode::Waiting(_)) {
            self.mode = Mode::Waiting(message);
        }
    }

    /// Lists the account signed in as, with what it signed in with, and
    /// picks it.
    pub fn signed_in(&mut self, address: String, login: Login) {
        match self
            .accounts
            .iter()
            .position(|(a, _)| a.eq_ignore_ascii_case(&address))
        {
            Some(at) => {
                self.accounts[at].1 = login;
                self.selected = at;
            }
            None => {
                self.accounts.push((address, login));
                self.selected = self.accounts.len() - 1;
            }
        }
        self.mode = Mode::List;
    }

    /// Back to the list, after a sign-in failed.
    pub fn sign_in_failed(&mut self) {
        self.mode = Mode::List;
    }

    pub fn key(&mut self, key: KeyEvent) -> Request {
        match &mut self.mode {
            Mode::List => self.list_key(key),
            Mode::Form { .. } => self.form_key(key),
            Mode::Waiting(_) => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::List;
                    Request::Cancel
                }
                _ => Request::None,
            },
        }
    }

    fn list_key(&mut self, key: KeyEvent) -> Request {
        let last = self.accounts.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Request::Close,
            KeyCode::Char('j') | KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('a') => self.open_form(None, self.defaults.clone()),
            _ => {
                let Some((address, login)) = self.picked().cloned() else {
                    return Request::None;
                };
                return match key.code {
                    KeyCode::Char('e') => {
                        self.open_form(Some(address), login);
                        Request::None
                    }
                    KeyCode::Char('r') => {
                        self.wait();
                        Request::SignIn {
                            login,
                            replacing: Some(address),
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete => Request::Remove(address),
                    KeyCode::Char('s') => Request::Signature(address),
                    KeyCode::Char('K') => self.move_by(address, -1),
                    KeyCode::Char('J') => self.move_by(address, 1),
                    _ => Request::None,
                };
            }
        }
        Request::None
    }

    fn move_by(&mut self, address: String, delta: isize) -> Request {
        let to = self
            .selected
            .saturating_add_signed(delta)
            .min(self.accounts.len() - 1);
        if to == self.selected {
            return Request::None;
        }
        let account = self.accounts.remove(self.selected);
        self.accounts.insert(to, account);
        self.selected = to;
        Request::Move { address, delta }
    }

    fn open_form(&mut self, editing: Option<String>, login: Login) {
        self.mode = Mode::Form {
            editing,
            tenant: TextInput::from(login.tenant.as_str()),
            client_id: TextInput::from(login.client_id.as_str()),
            field: Field::Tenant,
        };
    }

    fn wait(&mut self) {
        self.mode = Mode::Waiting("Asking Microsoft for a sign-in code…".to_owned());
    }

    fn form_key(&mut self, key: KeyEvent) -> Request {
        let Mode::Form {
            editing,
            tenant,
            client_id,
            field,
        } = &mut self.mode
        else {
            return Request::None;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down => {
                *field = match field {
                    Field::Tenant => Field::ClientId,
                    Field::ClientId => Field::Tenant,
                };
            }
            KeyCode::Enter => {
                let login = Login {
                    tenant: tenant.text().trim().to_owned(),
                    client_id: client_id.text().trim().to_owned(),
                };
                if login.tenant.is_empty() || login.client_id.is_empty() {
                    return Request::None;
                }
                let replacing = editing.take();
                self.wait();
                return Request::SignIn { login, replacing };
            }
            _ => {
                match field {
                    Field::Tenant => tenant.key(key),
                    Field::ClientId => client_id.key(key),
                };
            }
        }
        Request::None
    }
}

impl AppState {
    /// M: opens the accounts, with the one being looked at picked.
    pub(crate) fn manage_accounts(&mut self) {
        let accounts = self
            .accounts
            .iter()
            .enumerate()
            .map(|(i, address)| {
                let login = self.logins.get(i).unwrap_or(&self.default_login);
                (address.clone(), login.clone())
            })
            .collect();
        let manager =
            AccountManager::new(accounts, self.current_account(), self.default_login.clone());
        self.dialog = Some(Dialog::Accounts(manager));
    }

    /// A key for the accounts dialog.
    pub(crate) fn accounts_key(
        &mut self,
        mut manager: AccountManager,
        key: KeyEvent,
    ) -> Option<Effect> {
        let request = manager.key(key);
        match request {
            Request::Close => {
                if self.accounts_changed {
                    self.reload_accounts(Reload::Keep);
                }
                return None;
            }
            Request::Remove(address) => {
                self.dialog = Some(Dialog::Confirm(Confirm::RemoveAccount { address }));
                return None;
            }
            Request::Signature(address) => {
                match self
                    .accounts
                    .iter()
                    .position(|a| a.eq_ignore_ascii_case(&address))
                {
                    Some(account) => self.edit_signature(account),
                    None => {
                        self.show_info("Close this dialog first to write its signature");
                        self.dialog = Some(Dialog::Accounts(manager));
                    }
                }
                return None;
            }
            _ => {}
        }
        self.dialog = Some(Dialog::Accounts(manager));
        match request {
            Request::SignIn { login, replacing } => Some(Effect::SignIn { login, replacing }),
            Request::Cancel => Some(Effect::CancelSignIn),
            Request::Move { address, delta } => {
                self.accounts_changed = true;
                Some(Effect::MoveAccount { address, delta })
            }
            _ => None,
        }
    }

    /// Closes the UI to start it again with the accounts changed.
    pub fn reload_accounts(&mut self, reload: Reload) {
        self.reload = Some(reload);
        self.should_quit = true;
    }

    /// Shows what Microsoft says to do to sign in.
    pub fn sign_in_code(&mut self, message: String) {
        if let Some(Dialog::Accounts(manager)) = &mut self.dialog {
            manager.show_code(message);
        }
    }

    /// Lists the account signed in as. It shows in the panes once the
    /// accounts dialog closes, or at once if it has closed already.
    pub fn signed_in(&mut self, address: String, login: Login) {
        self.show_info(format!("Signed in as {address}"));
        self.accounts_changed = true;
        match &mut self.dialog {
            Some(Dialog::Accounts(manager)) => manager.signed_in(address, login),
            _ => self.reload_accounts(Reload::Keep),
        }
    }

    pub fn sign_in_failed(&mut self, reason: &str) {
        self.show_error(format!("Could not sign in: {reason}"));
        if let Some(Dialog::Accounts(manager)) = &mut self.dialog {
            manager.sign_in_failed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(tenant: &str) -> Login {
        Login {
            tenant: tenant.to_owned(),
            client_id: "app".to_owned(),
        }
    }

    fn manager() -> AccountManager {
        AccountManager::new(
            vec![
                ("me@outlook.com".to_owned(), login("consumers")),
                ("me@work.example".to_owned(), login("organizations")),
            ],
            0,
            login("consumers"),
        )
    }

    fn press(manager: &mut AccountManager, keys: &[KeyCode]) -> Request {
        let mut request = Request::None;
        for &code in keys {
            request = manager.key(KeyEvent::from(code));
        }
        request
    }

    fn chars(text: &str) -> Vec<KeyCode> {
        text.chars().map(KeyCode::Char).collect()
    }

    #[test]
    fn j_and_k_pick_an_account_and_stop_at_the_ends() {
        let mut manager = manager();

        press(&mut manager, &[KeyCode::Char('j'), KeyCode::Char('j')]);
        assert_eq!(manager.selected, 1);
        press(&mut manager, &[KeyCode::Char('k'), KeyCode::Char('k')]);
        assert_eq!(manager.selected, 0);
    }

    #[test]
    fn a_signs_in_a_new_account_with_the_defaults_or_what_is_typed() {
        let mut manager = manager();

        press(&mut manager, &[KeyCode::Char('a')]);
        assert!(manager.takes_text());
        let keys = [
            vec![KeyCode::Tab, KeyCode::End],
            chars("2"),
            vec![KeyCode::Enter],
        ]
        .concat();
        let request = press(&mut manager, &keys);

        assert_eq!(
            request,
            Request::SignIn {
                login: Login {
                    tenant: "consumers".to_owned(),
                    client_id: "app2".to_owned(),
                },
                replacing: None,
            }
        );
        assert!(matches!(manager.mode, Mode::Waiting(_)));
    }

    #[test]
    fn e_signs_the_picked_account_in_with_another_tenant() {
        let mut manager = manager();

        press(&mut manager, &[KeyCode::Char('j'), KeyCode::Char('e')]);
        let mut keys = vec![KeyCode::Backspace; "organizations".len()];
        keys.extend(chars("contoso"));
        keys.push(KeyCode::Enter);
        let request = press(&mut manager, &keys);

        assert_eq!(
            request,
            Request::SignIn {
                login: login("contoso"),
                replacing: Some("me@work.example".to_owned()),
            }
        );
    }

    #[test]
    fn an_empty_field_is_not_signed_in_with() {
        let mut manager = manager();
        press(&mut manager, &[KeyCode::Char('a')]);

        let mut keys = vec![KeyCode::Backspace; "consumers".len()];
        keys.push(KeyCode::Enter);

        assert_eq!(press(&mut manager, &keys), Request::None);
        assert!(manager.takes_text());
    }

    #[test]
    fn esc_leaves_the_form_then_the_dialog() {
        let mut manager = manager();
        press(&mut manager, &[KeyCode::Char('a')]);

        assert_eq!(press(&mut manager, &[KeyCode::Esc]), Request::None);
        assert_eq!(manager.mode, Mode::List);
        assert_eq!(press(&mut manager, &[KeyCode::Esc]), Request::Close);
    }

    #[test]
    fn r_signs_the_picked_account_in_again() {
        let mut manager = manager();

        let request = press(&mut manager, &[KeyCode::Char('r')]);

        assert_eq!(
            request,
            Request::SignIn {
                login: login("consumers"),
                replacing: Some("me@outlook.com".to_owned()),
            }
        );
    }

    #[test]
    fn esc_while_waiting_cancels_the_sign_in() {
        let mut manager = manager();
        press(&mut manager, &[KeyCode::Char('r')]);

        manager.show_code("Go to example.com and enter ABC".to_owned());
        assert_eq!(
            manager.mode,
            Mode::Waiting("Go to example.com and enter ABC".to_owned())
        );

        assert_eq!(press(&mut manager, &[KeyCode::Esc]), Request::Cancel);
        assert_eq!(manager.mode, Mode::List);
    }

    #[test]
    fn a_code_arriving_after_cancelling_is_not_shown() {
        let mut manager = manager();

        manager.show_code("late".to_owned());

        assert_eq!(manager.mode, Mode::List);
    }

    #[test]
    fn d_asks_to_remove_and_s_writes_the_signature() {
        let mut manager = manager();
        press(&mut manager, &[KeyCode::Char('j')]);

        assert_eq!(
            press(&mut manager, &[KeyCode::Char('d')]),
            Request::Remove("me@work.example".to_owned())
        );
        assert_eq!(
            press(&mut manager, &[KeyCode::Char('s')]),
            Request::Signature("me@work.example".to_owned())
        );
    }

    #[test]
    fn shift_j_and_k_move_the_picked_account() {
        let mut manager = manager();

        let request = press(&mut manager, &[KeyCode::Char('J')]);

        assert_eq!(
            request,
            Request::Move {
                address: "me@outlook.com".to_owned(),
                delta: 1
            }
        );
        assert_eq!(manager.accounts[1].0, "me@outlook.com");
        assert_eq!(manager.selected, 1);
        assert_eq!(press(&mut manager, &[KeyCode::Char('J')]), Request::None);
    }

    #[test]
    fn a_new_account_signed_in_is_listed_and_picked() {
        let mut manager = manager();
        press(&mut manager, &[KeyCode::Char('a'), KeyCode::Enter]);

        manager.signed_in("new@outlook.com".to_owned(), login("consumers"));

        assert_eq!(manager.accounts.len(), 3);
        assert_eq!(manager.picked().unwrap().0, "new@outlook.com");
        assert_eq!(manager.mode, Mode::List);
    }

    #[test]
    fn signing_in_again_updates_the_account_listed() {
        let mut manager = manager();

        manager.signed_in("ME@work.example".to_owned(), login("contoso"));

        assert_eq!(manager.accounts.len(), 2);
        assert_eq!(manager.accounts[1].1, login("contoso"));
        assert_eq!(manager.selected, 1);
    }

    mod in_the_app {
        use super::*;
        use crate::model::Action;
        use crate::tui::keymap::Keymap;

        fn app() -> AppState {
            AppState {
                accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
                signatures: vec![String::new(), String::new()],
                logins: vec![login("consumers"), login("organizations")],
                default_login: login("consumers"),
                ..AppState::default()
            }
        }

        fn key(app: &mut AppState, code: KeyCode) -> Option<Effect> {
            app.on_key(KeyEvent::from(code), &mut Keymap::default())
        }

        fn manager(app: &AppState) -> &AccountManager {
            match &app.dialog {
                Some(Dialog::Accounts(manager)) => manager,
                other => panic!("no accounts dialog: {other:?}"),
            }
        }

        #[test]
        fn shift_m_lists_the_accounts_with_what_each_signs_in_with() {
            let mut app = app();

            app.update(Action::ManageAccounts);

            assert_eq!(manager(&app).accounts[1].1, login("organizations"));
        }

        #[test]
        fn removing_asks_first_then_closes_the_ui_to_forget_it() {
            let mut app = app();
            app.update(Action::ManageAccounts);

            key(&mut app, KeyCode::Char('d'));
            assert_eq!(
                app.confirm(),
                Some(&Confirm::RemoveAccount {
                    address: "me@outlook.com".to_owned()
                })
            );
            let effect = key(&mut app, KeyCode::Char('y'));

            assert_eq!(
                effect,
                Some(Effect::RemoveAccount("me@outlook.com".to_owned()))
            );
            app.reload_accounts(Reload::Remove("me@outlook.com".to_owned()));
            assert!(app.should_quit);
        }

        #[test]
        fn signing_in_shows_the_code_then_lists_the_account() {
            let mut app = app();
            app.update(Action::ManageAccounts);

            let effect = key(&mut app, KeyCode::Char('a'));
            assert_eq!(effect, None);
            let effect = key(&mut app, KeyCode::Enter);
            assert_eq!(
                effect,
                Some(Effect::SignIn {
                    login: login("consumers"),
                    replacing: None
                })
            );
            app.sign_in_code("Enter ABC".to_owned());
            assert_eq!(manager(&app).mode, Mode::Waiting("Enter ABC".to_owned()));

            app.signed_in("new@outlook.com".to_owned(), login("consumers"));

            assert_eq!(manager(&app).accounts.len(), 3);
            assert!(!app.should_quit, "shown once the dialog closes");
        }

        #[test]
        fn closing_after_a_change_shows_the_accounts_again() {
            let mut app = app();
            app.update(Action::ManageAccounts);

            let effect = key(&mut app, KeyCode::Char('J'));
            assert_eq!(
                effect,
                Some(Effect::MoveAccount {
                    address: "me@outlook.com".to_owned(),
                    delta: 1
                })
            );
            key(&mut app, KeyCode::Esc);

            assert_eq!(app.reload, Some(Reload::Keep));
            assert!(app.should_quit);
        }

        #[test]
        fn closing_with_nothing_changed_just_closes() {
            let mut app = app();
            app.update(Action::ManageAccounts);

            key(&mut app, KeyCode::Esc);

            assert_eq!(app.dialog, None);
            assert!(!app.should_quit);
        }

        #[test]
        fn a_failed_sign_in_says_why_and_goes_back_to_the_list() {
            let mut app = app();
            app.update(Action::ManageAccounts);
            key(&mut app, KeyCode::Char('r'));

            app.sign_in_failed("denied");

            assert_eq!(manager(&app).mode, Mode::List);
            assert_eq!(
                app.status,
                Some(crate::tui::app::Status::Error(
                    "Could not sign in: denied".to_owned()
                ))
            );
        }

        #[test]
        fn esc_while_waiting_cancels_the_sign_in() {
            let mut app = app();
            app.update(Action::ManageAccounts);
            key(&mut app, KeyCode::Char('r'));

            assert_eq!(key(&mut app, KeyCode::Esc), Some(Effect::CancelSignIn));
        }

        #[test]
        fn s_writes_the_picked_accounts_signature() {
            let mut app = app();
            app.update(Action::ManageAccounts);

            key(&mut app, KeyCode::Char('j'));
            key(&mut app, KeyCode::Char('s'));

            assert_eq!(app.dialog, None);
            assert_eq!(app.screen.signature().unwrap().account, 1);
        }

        #[test]
        fn a_sign_in_finishing_after_the_dialog_closed_shows_at_once() {
            let mut app = app();

            app.signed_in("new@outlook.com".to_owned(), login("consumers"));

            assert_eq!(app.reload, Some(Reload::Keep));
        }
    }

    #[test]
    fn with_no_accounts_only_adding_does_anything() {
        let mut manager = AccountManager::new(Vec::new(), 3, login("consumers"));

        for key in ['j', 'e', 'r', 'd', 's', 'J'] {
            assert_eq!(press(&mut manager, &[KeyCode::Char(key)]), Request::None);
        }
        press(&mut manager, &[KeyCode::Char('a')]);
        assert!(manager.takes_text());
    }
}
