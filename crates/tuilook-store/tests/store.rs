use tuilook_store::Store;

#[test]
fn new_store_is_empty() {
    let store = Store::open_in_memory().unwrap();

    assert!(store.folders().unwrap().is_empty());
}
