use tuilook_model::{Folder, FolderId, WellKnown};
use tuilook_store::Store;

fn folder(id: &str, name: &str, well_known: Option<WellKnown>) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: None,
        name: name.to_owned(),
        well_known,
        unread: 0,
        total: 0,
    }
}

#[test]
fn new_store_is_empty() {
    let store = Store::open_in_memory().unwrap();

    assert!(store.folders().unwrap().is_empty());
}

#[test]
fn upserted_folders_are_returned_in_tree_order() {
    let mut store = Store::open_in_memory().unwrap();
    let mut child = folder("f2", "Receipts", None);
    child.parent_id = Some(FolderId("f1".to_owned()));
    child.unread = 3;
    child.total = 10;

    store
        .upsert_folders(&[child.clone(), folder("f1", "Inbox", Some(WellKnown::Inbox))])
        .unwrap();

    assert_eq!(
        store.folders().unwrap(),
        [folder("f1", "Inbox", Some(WellKnown::Inbox)), child]
    );
}

#[test]
fn upserting_an_existing_folder_updates_it() {
    let mut store = Store::open_in_memory().unwrap();
    store.upsert_folders(&[folder("f1", "Old", None)]).unwrap();

    store.upsert_folders(&[folder("f1", "New", None)]).unwrap();

    assert_eq!(store.folders().unwrap(), [folder("f1", "New", None)]);
}
