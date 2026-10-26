use std::sync::Mutex;

/// Where the refresh token lives between runs.
pub trait SecretStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, SecretError>;
    fn save(&self, secret: &str) -> Result<(), SecretError>;
    fn clear(&self) -> Result<(), SecretError>;
}

#[derive(Debug, thiserror::Error)]
#[error("credential store: {0}")]
pub struct SecretError(pub String);

/// The OS keychain: macOS Keychain, Windows Credential Manager or Secret Service.
pub struct KeyringStore {
    entry: keyring::Entry,
}

impl KeyringStore {
    pub fn new(account: &str) -> Result<Self, SecretError> {
        let entry = keyring::Entry::new("tuilook", account).map_err(to_secret_error)?;
        Ok(Self { entry })
    }
}

impl SecretStore for KeyringStore {
    fn load(&self) -> Result<Option<String>, SecretError> {
        match self.entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(to_secret_error(e)),
        }
    }

    fn save(&self, secret: &str) -> Result<(), SecretError> {
        self.entry.set_password(secret).map_err(to_secret_error)
    }

    fn clear(&self) -> Result<(), SecretError> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(to_secret_error(e)),
        }
    }
}

fn to_secret_error(e: keyring::Error) -> SecretError {
    SecretError(e.to_string())
}

/// Keeps the secret in memory only.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<String>>);

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<String>, SecretError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, secret: &str) -> Result<(), SecretError> {
        *self.0.lock().unwrap() = Some(secret.to_owned());
        Ok(())
    }

    fn clear(&self) -> Result<(), SecretError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
