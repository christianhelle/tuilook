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

/// Characters kept in one keychain entry. Windows Credential Manager holds
/// at most 2560 bytes, and keeps text as UTF-16, so 1280 characters; work
/// and school refresh tokens are often longer.
const PIECE: usize = 1000;

/// Starts the first entry of a secret split over several, followed by how
/// many pieces there are. Refresh tokens never start like this.
const PIECES: &str = "tuilook-pieces:";

/// Numbered places to keep text in: 0 is the secret's own entry, and the
/// pieces of a long secret follow it.
trait Slots {
    fn get(&self, slot: usize) -> Result<Option<String>, SecretError>;
    fn set(&self, slot: usize, text: &str) -> Result<(), SecretError>;
    fn delete(&self, slot: usize) -> Result<(), SecretError>;
}

/// How many pieces the secret in `slots` is split over, if it is.
fn pieces_in(first: Option<&str>) -> usize {
    first
        .and_then(|text| text.strip_prefix(PIECES))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

fn load(slots: &impl Slots) -> Result<Option<String>, SecretError> {
    let Some(first) = slots.get(0)? else {
        return Ok(None);
    };
    let count = pieces_in(Some(&first));
    if count == 0 {
        return Ok(Some(first));
    }
    let mut secret = String::new();
    for slot in 1..=count {
        match slots.get(slot)? {
            Some(piece) => secret.push_str(&piece),
            // Half a token is no token; signing in again mends it.
            None => return Ok(None),
        }
    }
    Ok(Some(secret))
}

fn save(slots: &impl Slots, secret: &str) -> Result<(), SecretError> {
    let before = pieces_in(slots.get(0)?.as_deref());
    let chars: Vec<char> = secret.chars().collect();
    let pieces: Vec<String> = if chars.len() <= PIECE {
        vec![]
    } else {
        chars.chunks(PIECE).map(|c| c.iter().collect()).collect()
    };
    for (index, piece) in pieces.iter().enumerate() {
        slots.set(index + 1, piece)?;
    }
    match pieces.len() {
        0 => slots.set(0, secret)?,
        count => slots.set(0, &format!("{PIECES}{count}"))?,
    }
    for slot in pieces.len() + 1..=before {
        slots.delete(slot)?;
    }
    Ok(())
}

fn clear(slots: &impl Slots) -> Result<(), SecretError> {
    let count = pieces_in(slots.get(0)?.as_deref());
    for slot in 1..=count {
        slots.delete(slot)?;
    }
    slots.delete(0)
}

/// The OS keychain: macOS Keychain, Windows Credential Manager or Secret
/// Service. A secret too long for one entry is split over several.
pub struct KeyringStore {
    account: String,
}

impl KeyringStore {
    pub fn new(account: &str) -> Result<Self, SecretError> {
        Ok(Self {
            account: account.to_owned(),
        })
    }

    fn entry(&self, slot: usize) -> Result<keyring::Entry, SecretError> {
        let user = match slot {
            0 => self.account.clone(),
            n => format!("{}#{n}", self.account),
        };
        keyring::Entry::new("tuilook", &user).map_err(to_secret_error)
    }
}

impl Slots for KeyringStore {
    fn get(&self, slot: usize) -> Result<Option<String>, SecretError> {
        match self.entry(slot)?.get_password() {
            Ok(text) => Ok(Some(text)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(to_secret_error(e)),
        }
    }

    fn set(&self, slot: usize, text: &str) -> Result<(), SecretError> {
        self.entry(slot)?
            .set_password(text)
            .map_err(to_secret_error)
    }

    fn delete(&self, slot: usize) -> Result<(), SecretError> {
        match self.entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(to_secret_error(e)),
        }
    }
}

impl SecretStore for KeyringStore {
    fn load(&self) -> Result<Option<String>, SecretError> {
        load(self)
    }

    fn save(&self, secret: &str) -> Result<(), SecretError> {
        save(self, secret)
    }

    fn clear(&self) -> Result<(), SecretError> {
        clear(self)
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Slots in memory that refuse text longer than one keychain entry holds.
    #[derive(Default)]
    struct MemorySlots(Mutex<HashMap<usize, String>>);

    impl Slots for MemorySlots {
        fn get(&self, slot: usize) -> Result<Option<String>, SecretError> {
            Ok(self.0.lock().unwrap().get(&slot).cloned())
        }

        fn set(&self, slot: usize, text: &str) -> Result<(), SecretError> {
            if text.encode_utf16().count() * 2 > 2560 {
                return Err(SecretError("longer than the platform limit".to_owned()));
            }
            self.0.lock().unwrap().insert(slot, text.to_owned());
            Ok(())
        }

        fn delete(&self, slot: usize) -> Result<(), SecretError> {
            self.0.lock().unwrap().remove(&slot);
            Ok(())
        }
    }

    fn slots_used(slots: &MemorySlots) -> Vec<usize> {
        let mut used: Vec<usize> = slots.0.lock().unwrap().keys().copied().collect();
        used.sort();
        used
    }

    #[test]
    fn a_short_secret_takes_one_entry() {
        let slots = MemorySlots::default();

        save(&slots, "short").unwrap();

        assert_eq!(slots_used(&slots), [0]);
        assert_eq!(load(&slots).unwrap().as_deref(), Some("short"));
    }

    #[test]
    fn a_long_secret_is_split_over_entries_that_fit() {
        let slots = MemorySlots::default();
        let token = "x".repeat(2 * PIECE + 1);

        save(&slots, &token).unwrap();

        assert_eq!(slots_used(&slots), [0, 1, 2, 3]);
        assert_eq!(load(&slots).unwrap(), Some(token));
    }

    #[test]
    fn a_shorter_secret_drops_the_pieces_it_no_longer_needs() {
        let slots = MemorySlots::default();
        save(&slots, &"x".repeat(3 * PIECE)).unwrap();

        save(&slots, &"y".repeat(PIECE + 1)).unwrap();
        assert_eq!(slots_used(&slots), [0, 1, 2]);
        save(&slots, "short").unwrap();

        assert_eq!(slots_used(&slots), [0]);
        assert_eq!(load(&slots).unwrap().as_deref(), Some("short"));
    }

    #[test]
    fn clearing_removes_every_piece() {
        let slots = MemorySlots::default();
        save(&slots, &"x".repeat(2 * PIECE)).unwrap();

        clear(&slots).unwrap();

        assert!(slots_used(&slots).is_empty());
        assert_eq!(load(&slots).unwrap(), None);
    }

    #[test]
    fn a_missing_piece_means_signed_out() {
        let slots = MemorySlots::default();
        save(&slots, &"x".repeat(2 * PIECE)).unwrap();

        slots.delete(2).unwrap();

        assert_eq!(load(&slots).unwrap(), None);
    }
}
