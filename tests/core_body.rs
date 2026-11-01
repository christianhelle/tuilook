use tuilook::core::{fake::FakeBackend, load_body, sync_folders, sync_messages};
use tuilook::model::{
    Attachment, AttachmentId, AttachmentKind, Body, BodyType, Folder, FolderId, Importance,
    Message, MessageId,
};
use tuilook::store::Store;

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
        pinned: false,
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

fn report() -> Attachment {
    Attachment {
        id: AttachmentId("a1".to_owned()),
        name: "report.pdf".to_owned(),
        content_type: "application/pdf".to_owned(),
        size: 1_200_000,
        kind: AttachmentKind::File,
    }
}

/// A backend whose message m1 has `report.pdf` attached.
fn backend_with_attachment() -> FakeBackend {
    let mut backend = backend_with_body("<p>See attached</p>");
    let mut m1 = message("m1");
    m1.has_attachments = true;
    backend.messages = [(FolderId("f1".to_owned()), vec![m1])].into();
    backend.attachments = [(MessageId("m1".to_owned()), vec![report()])].into();
    backend
}

#[tokio::test]
async fn opening_a_message_with_attachments_lists_them_too() {
    let backend = backend_with_attachment();
    let mut store = synced(&backend).await;

    let (text, attachments) = load_body(&backend, &mut store, &MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert_eq!(text, "See attached");
    assert_eq!(attachments, [report()]);
}

#[tokio::test]
async fn attachments_are_served_from_the_cache_with_the_body() {
    let mut backend = backend_with_attachment();
    let mut store = synced(&backend).await;
    let id = MessageId("m1".to_owned());
    load_body(&backend, &mut store, &id).await.unwrap();

    backend.attachments.clear();
    let (_, attachments) = load_body(&backend, &mut store, &id).await.unwrap();

    assert_eq!(attachments, [report()]);
}

#[tokio::test]
async fn a_message_without_attachments_does_not_ask_for_them() {
    let backend = backend_with_body("<p>Hi</p>");
    let mut store = synced(&backend).await;

    let (_, attachments) = load_body(&backend, &mut store, &MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert!(attachments.is_empty());
    assert!(backend.listed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn opening_a_message_fetches_its_body_as_text() {
    let backend = backend_with_body("<p>Hello <b>there</b></p>");
    let mut store = synced(&backend).await;

    let (text, _) = load_body(&backend, &mut store, &MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert_eq!(text, "Hello **there**");
}

#[tokio::test]
async fn a_fetched_body_is_served_from_the_cache_afterwards() {
    let mut backend = backend_with_body("<p>Cached</p>");
    let mut store = synced(&backend).await;
    let id = MessageId("m1".to_owned());
    load_body(&backend, &mut store, &id).await.unwrap();

    backend.bodies.clear();
    let (text, _) = load_body(&backend, &mut store, &id).await.unwrap();

    assert_eq!(text, "Cached");
}
