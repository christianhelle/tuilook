//! Managing the signed-in accounts: adding one, signing one in again or
//! with another tenant or app, writing its signature, moving it up or down
//! and removing it.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::tui::input::TextInput;

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
    /// Write the signature of the account at this index.
    Signature(usize),
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
                    KeyCode::Char('s') => Request::Signature(self.selected),
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
            Request::Signature(1)
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
    fn with_no_accounts_only_adding_does_anything() {
        let mut manager = AccountManager::new(Vec::new(), 3, login("consumers"));

        for key in ['j', 'e', 'r', 'd', 's', 'J'] {
            assert_eq!(press(&mut manager, &[KeyCode::Char(key)]), Request::None);
        }
        press(&mut manager, &[KeyCode::Char('a')]);
        assert!(manager.takes_text());
    }
}
