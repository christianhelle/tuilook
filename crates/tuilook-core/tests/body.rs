use tuilook_core::{fake::FakeBackend, load_body, sync_folders, sync_messages};
use tuilook_model::{Body, BodyType, Folder, FolderId, Importance, Message, MessageId};
use tuilook_store::Store;

fn message(id: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("f1".to_owned()),
        conversation_id: String::new(),
        subject: String::new(),
        from_name: String::new(),
        from_addr: String::new(),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        is_read: false,
        flagged: false,
        importance: Importance::Normal,
        has_attachments: false,
        preview: String::new(),
    }
}

async fn synced(backend: &FakeBackend) -> Store {
    let mut store = Store::open_in_memory().unwrap();
    sync_folders(backend, &mut store).await.unwrap();
    sync_messages(backend, &mut store, &FolderId("f1".to_owned()))
        .await
        .unwrap();
    store
}

fn backend_with_body(content: &str) -> FakeBackend {
    FakeBackend {
        folders: vec![Folder {
            id: FolderId("f1".to_owned()),
            parent_id: None,
            name: "Inbox".to_owned(),
            well_known: None,
            unread: 0,
            total: 1,
        }],
        messages: [(FolderId("f1".to_owned()), vec![message("m1")])].into(),
        bodies: [(
            MessageId("m1".to_owned()),
            Body {
                content_type: BodyType::Html,
                content: content.to_owned(),
            },
        )]
        .into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn opening_a_message_fetches_its_body_as_text() {
    let backend = backend_with_body("<p>Hello <b>there</b></p>");
    let mut store = synced(&backend).await;

    let text = load_body(&backend, &mut store, &MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert_eq!(text, "Hello **there**\n");
}

#[tokio::test]
async fn a_fetched_body_is_served_from_the_cache_afterwards() {
    let mut backend = backend_with_body("<p>Cached</p>");
    let mut store = synced(&backend).await;
    let id = MessageId("m1".to_owned());
    load_body(&backend, &mut store, &id).await.unwrap();

    backend.bodies.clear();
    let text = load_body(&backend, &mut store, &id).await.unwrap();

    assert_eq!(text, "Cached\n");
}
