//! The accounts signed in to, and how each one signs in again.

use std::{io, path::Path};

use serde::{Deserialize, Serialize};

/// One mailbox, known by its email address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub address: String,
    /// The tenant and app registration it signed in with, used to refresh
    /// its sign-in whatever the command line says now.
    pub tenant: String,
    pub client_id: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FindError {
    #[error("no signed-in account matches {0}; see `otlk accounts`")]
    NoMatch(String),
    #[error("{0} matches more than one account: {list}", list = .1.join(", "))]
    Ambiguous(String, Vec<String>),
}

/// The signed-in accounts, in the order they signed in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accounts(pub Vec<Account>);

impl Accounts {
    /// The accounts saved at `path`, or none when nothing is saved there.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Adds `account`, or replaces the one with the same address.
    pub fn upsert(&mut self, account: Account) {
        match self
            .0
            .iter_mut()
            .find(|a| same(&a.address, &account.address))
        {
            Some(known) => *known = account,
            None => self.0.push(account),
        }
    }

    pub fn remove(&mut self, address: &str) {
        self.0.retain(|a| !same(&a.address, address));
    }

    /// The account whose address is `query`, or the only one starting with it.
    pub fn find(&self, query: &str) -> Result<&Account, FindError> {
        if let Some(account) = self.0.iter().find(|a| same(&a.address, query)) {
            return Ok(account);
        }
        let query_lower = query.to_lowercase();
        let matches: Vec<&Account> = self
            .0
            .iter()
            .filter(|a| a.address.to_lowercase().starts_with(&query_lower))
            .collect();
        match matches[..] {
            [account] => Ok(account),
            [] => Err(FindError::NoMatch(query.to_owned())),
            _ => Err(FindError::Ambiguous(
                query.to_owned(),
                matches.iter().map(|a| a.address.clone()).collect(),
            )),
        }
    }
}

/// Email addresses are compared without regard to case.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(address: &str) -> Account {
        Account {
            address: address.to_owned(),
            tenant: "consumers".to_owned(),
            client_id: "app".to_owned(),
        }
    }

    fn accounts() -> Accounts {
        Accounts(vec![
            account("me@outlook.com"),
            account("me@work.example"),
            account("other@outlook.com"),
        ])
    }

    #[test]
    fn saves_and_loads_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");

        accounts().save(&path).unwrap();

        assert_eq!(Accounts::load(&path).unwrap(), accounts());
    }

    #[test]
    fn nothing_saved_means_no_accounts() {
        let dir = tempfile::tempdir().unwrap();

        let loaded = Accounts::load(&dir.path().join("accounts.json")).unwrap();

        assert!(loaded.0.is_empty());
    }

    #[test]
    fn signing_in_again_replaces_the_account() {
        let mut accounts = accounts();
        let again = Account {
            tenant: "organizations".to_owned(),
            ..account("ME@work.example")
        };

        accounts.upsert(again.clone());

        assert_eq!(accounts.0.len(), 3);
        assert_eq!(accounts.0[1], again);
    }

    #[test]
    fn finds_by_address_or_a_unique_start() {
        let accounts = accounts();

        assert_eq!(
            accounts.find("ME@OUTLOOK.COM").unwrap().address,
            "me@outlook.com"
        );
        assert_eq!(accounts.find("me@w").unwrap().address, "me@work.example");
        assert_eq!(accounts.find("oth").unwrap().address, "other@outlook.com");
    }

    #[test]
    fn a_start_shared_by_several_is_ambiguous() {
        assert_eq!(
            accounts().find("me"),
            Err(FindError::Ambiguous(
                "me".to_owned(),
                vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()]
            ))
        );
        assert_eq!(
            accounts().find("nobody"),
            Err(FindError::NoMatch("nobody".to_owned()))
        );
    }

    #[test]
    fn removes_by_address() {
        let mut accounts = accounts();

        accounts.remove("Me@Outlook.com");

        assert_eq!(accounts.0.len(), 2);
        assert!(accounts.find("me@outlook.com").is_err());
    }
}
