use tuilook_core::{fake::FakeBackend, sync_folders};
use tuilook_model::{Folder, FolderId, WellKnown};
use tuilook_store::Store;

fn folder(id: &str, name: &str, well_known: Option<WellKnown>) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: None,
        name: name.to_owned(),
        well_known,
        unread: 1,
        total: 2,
    }
}

#[tokio::test]
async fn syncing_folders_stores_the_backend_folder_tree() {
    let backend = FakeBackend {
        folders: vec![
            folder("f2", "Projects", None),
            folder("f1", "Inbox", Some(WellKnown::Inbox)),
        ],
        ..Default::default()
    };
    let mut store = Store::open_in_memory().unwrap();

    sync_folders(&backend, &mut store).await.unwrap();

    assert_eq!(
        store.folders().unwrap(),
        [
            folder("f1", "Inbox", Some(WellKnown::Inbox)),
            folder("f2", "Projects", None)
        ]
    );
}
