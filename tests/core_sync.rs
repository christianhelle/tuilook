use tuilook::core::{fake::FakeBackend, sync_folders, sync_messages};
use tuilook::model::{
    BackendError, DeltaPage, Folder, FolderId, Importance, Message, MessageChange, MessageId,
    WellKnown,
};
use tuilook::store::Store;

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
        pinned: false,
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

fn delta(upserted: Vec<Message>, removed: &[&str], link: &str) -> DeltaPage {
    DeltaPage {
        upserted,
        changed: vec![],
        removed: removed
            .iter()
            .map(|id| MessageId((*id).to_owned()))
            .collect(),
        delta_link: Some(link.to_owned()),
    }
}

#[tokio::test]
async fn later_syncs_resume_from_the_stored_delta_link() {
    let inbox = FolderId("f1".to_owned());
    let m1 = message("m1", "2026-01-01T09:00:00Z");
    let m2 = message("m2", "2026-01-02T09:00:00Z");
    let m3 = message("m3", "2026-01-03T09:00:00Z");
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        deltas: [
            (
                (inbox.clone(), None),
                Ok(delta(vec![m1.clone(), m2.clone()], &[], "d1")),
            ),
            (
                (inbox.clone(), Some("d1".to_owned())),
                Ok(delta(vec![m3.clone()], &["m1"], "d2")),
            ),
        ]
        .into(),
        ..Default::default()
    };
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(&backend, &mut store).await.unwrap();

    sync_messages(&backend, &mut store, &inbox).await.unwrap();
    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [m3, m2]);
    assert_eq!(store.delta_link(&inbox).unwrap().as_deref(), Some("d2"));
}

#[tokio::test]
async fn a_partial_change_keeps_the_rest_of_the_message() {
    let inbox = FolderId("f1".to_owned());
    let m1 = message("m1", "2026-01-01T09:00:00Z");
    let read_elsewhere = DeltaPage {
        changed: vec![MessageChange {
            is_read: Some(true),
            ..MessageChange::new(m1.id.clone())
        }],
        ..delta(vec![], &[], "d2")
    };
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        deltas: [
            (
                (inbox.clone(), None),
                Ok(delta(vec![m1.clone()], &[], "d1")),
            ),
            ((inbox.clone(), Some("d1".to_owned())), Ok(read_elsewhere)),
        ]
        .into(),
        ..Default::default()
    };
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(&backend, &mut store).await.unwrap();

    sync_messages(&backend, &mut store, &inbox).await.unwrap();
    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    assert_eq!(
        store.messages_in(&inbox).unwrap(),
        [Message {
            is_read: true,
            ..m1
        }]
    );
}

#[tokio::test]
async fn an_expired_delta_link_triggers_a_full_resync() {
    let inbox = FolderId("f1".to_owned());
    let kept = message("m1", "2026-01-01T09:00:00Z");
    let gone = message("m2", "2026-01-02T09:00:00Z");
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        deltas: [
            (
                (inbox.clone(), Some("old".to_owned())),
                Err(BackendError::DeltaExpired),
            ),
            (
                (inbox.clone(), None),
                Ok(delta(vec![kept.clone()], &[], "fresh")),
            ),
        ]
        .into(),
        ..Default::default()
    };
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(&backend, &mut store).await.unwrap();
    store.upsert_messages(&[kept.clone(), gone]).unwrap();
    store.set_delta_link(&inbox, Some("old")).unwrap();

    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [kept]);
    assert_eq!(store.delta_link(&inbox).unwrap().as_deref(), Some("fresh"));
}

#[tokio::test]
async fn a_folder_deleted_on_the_server_is_forgotten_with_its_messages() {
    let projects = FolderId("f2".to_owned());
    let mut store = Store::open_in_memory().unwrap();
    let before = FakeBackend {
        folders: vec![
            folder("f1", "Inbox", Some(WellKnown::Inbox)),
            folder("f2", "Projects", None),
        ],
        messages: [(
            projects.clone(),
            vec![Message {
                folder_id: projects.clone(),
                ..message("m1", "2026-01-01T09:00:00Z")
            }],
        )]
        .into(),
        ..Default::default()
    };
    sync_folders(&before, &mut store).await.unwrap();
    sync_messages(&before, &mut store, &projects).await.unwrap();
    let after = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        ..Default::default()
    };

    sync_folders(&after, &mut store).await.unwrap();

    assert_eq!(
        store.folders().unwrap(),
        [folder("f1", "Inbox", Some(WellKnown::Inbox))]
    );
    assert!(store.messages_in(&projects).unwrap().is_empty());
}

#[tokio::test]
async fn folders_are_kept_when_listing_them_fails() {
    let mut store = Store::open_in_memory().unwrap();
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        ..Default::default()
    };
    sync_folders(&backend, &mut store).await.unwrap();
    let failing = FakeBackend {
        folders_error: Some(BackendError::Other("offline".to_owned())),
        ..Default::default()
    };

    assert!(sync_folders(&failing, &mut store).await.is_err());

    assert_eq!(store.folders().unwrap().len(), 1);
}

#[tokio::test]
async fn syncing_a_folder_brings_its_pins_even_from_past_the_sync_window() {
    let inbox = FolderId("f1".to_owned());
    let old = message("m0", "2025-01-01T09:00:00Z");
    let m1 = message("m1", "2026-01-01T09:00:00Z");
    let m2 = message("m2", "2026-01-02T09:00:00Z");
    let backend = FakeBackend {
        folders: vec![folder("f1", "Inbox", Some(WellKnown::Inbox))],
        messages: [(inbox.clone(), vec![m1.clone(), m2.clone()])].into(),
        older: [(inbox.clone(), vec![old.clone()])].into(),
        ..Default::default()
    };
    let pins = ["m0", "m1"].map(|id| MessageId(id.to_owned()));
    backend.pinned.lock().unwrap().extend(pins);
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(&backend, &mut store).await.unwrap();

    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    let pinned = |m: &Message| Message {
        pinned: true,
        ..m.clone()
    };
    assert_eq!(
        store.messages_in(&inbox).unwrap(),
        [pinned(&m1), pinned(&old), m2.clone()]
    );

    backend
        .pinned
        .lock()
        .unwrap()
        .remove(&MessageId("m1".to_owned()));
    sync_messages(&backend, &mut store, &inbox).await.unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [pinned(&old), m2, m1]);
}
