use tuilook_core::{fake::FakeBackend, sync_folders, sync_messages};
use tuilook_model::{Folder, FolderId, Importance, Message, MessageId, WellKnown};
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

fn message(id: &str, received_at: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("f1".to_owned()),
        conversation_id: String::new(),
        subject: format!("Subject {id}"),
        from_name: "Ada".to_owned(),
        from_addr: "ada@example.com".to_owned(),
        received_at: received_at.parse().unwrap(),
        is_read: false,
        flagged: false,
        importance: Importance::Normal,
        has_attachments: false,
        preview: String::new(),
    }
}

#[tokio::test]
async fn syncing_a_folder_stores_its_messages() {
    let inbox = FolderId("f1".to_owned());
    let older = message("m1", "2026-01-01T09:00:00Z");
    let newer = message("m2", "2026-01-02T09:00:00Z");
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        messages: [(inbox.clone(), vec![older.clone(), newer.clone()])].into(),
        ..Default::default()
    };
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(&backend, &mut store).await.unwrap();

    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [newer, older]);
}
