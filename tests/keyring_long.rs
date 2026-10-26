use tuilook::auth::{KeyringStore, SecretStore};

/// Uses the real OS keychain, so it is run by hand:
/// `cargo test --test keyring_long -- --ignored`.
#[test]
#[ignore]
fn a_token_longer_than_one_entry_round_trips_through_the_keychain() {
    let store = KeyringStore::new("tuilook-selftest").unwrap();
    let token = "t".repeat(3000);

    store.save(&token).unwrap();
    let loaded = store.load().unwrap();
    store.clear().unwrap();

    assert_eq!(loaded, Some(token));
    assert_eq!(store.load().unwrap(), None);
}
