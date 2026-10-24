use tuilook_model::{Body, BodyType, Folder, FolderId, Importance, Message, MessageId, WellKnown};
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

fn message(id: &str, folder_id: &str, received_at: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId(folder_id.to_owned()),
        conversation_id: "conv-1".to_owned(),
        subject: format!("Subject {id}"),
        from_name: "Ada Lovelace".to_owned(),
        from_addr: "ada@example.com".to_owned(),
        received_at: received_at.parse().unwrap(),
        is_read: true,
        flagged: true,
        importance: Importance::High,
        has_attachments: true,
        preview: "Hello there".to_owned(),
    }
}

#[test]
fn messages_in_a_folder_are_returned_newest_first() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .upsert_folders(&[
            folder("inbox", "Inbox", None),
            folder("other", "Other", None),
        ])
        .unwrap();
    let old = message("m1", "inbox", "2026-01-01T09:00:00Z");
    let new = message("m2", "inbox", "2026-02-01T09:00:00Z");
    let elsewhere = message("m3", "other", "2026-03-01T09:00:00Z");

    store
        .upsert_messages(&[old.clone(), elsewhere, new.clone()])
        .unwrap();

    let inbox = store.messages_in(&FolderId("inbox".to_owned())).unwrap();
    assert_eq!(inbox, [new, old]);
}

#[test]
fn upserting_an_existing_message_updates_it() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    let mut m = message("m1", "inbox", "2026-01-01T09:00:00Z");
    store.upsert_messages(&[m.clone()]).unwrap();

    m.is_read = false;
    m.importance = Importance::Low;
    store.upsert_messages(&[m.clone()]).unwrap();

    assert_eq!(
        store.messages_in(&FolderId("inbox".to_owned())).unwrap(),
        [m]
    );
}

#[test]
fn a_folder_remembers_its_delta_link() {
    let mut store = Store::open_in_memory().unwrap();
    let inbox = FolderId("inbox".to_owned());
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    assert_eq!(store.delta_link(&inbox).unwrap(), None);

    store
        .set_delta_link(&inbox, Some("https://delta/1"))
        .unwrap();

    assert_eq!(
        store.delta_link(&inbox).unwrap().as_deref(),
        Some("https://delta/1")
    );
}

#[test]
fn refreshing_folders_keeps_their_delta_links() {
    let mut store = Store::open_in_memory().unwrap();
    let inbox = FolderId("inbox".to_owned());
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    store
        .set_delta_link(&inbox, Some("https://delta/1"))
        .unwrap();

    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();

    assert_eq!(
        store.delta_link(&inbox).unwrap().as_deref(),
        Some("https://delta/1")
    );
}

#[test]
fn deleted_messages_are_gone() {
    let mut store = Store::open_in_memory().unwrap();
    let inbox = FolderId("inbox".to_owned());
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    let keep = message("m1", "inbox", "2026-01-01T09:00:00Z");
    let drop = message("m2", "inbox", "2026-01-02T09:00:00Z");
    store
        .upsert_messages(&[keep.clone(), drop.clone()])
        .unwrap();

    store.delete_messages(&[drop.id]).unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [keep]);
}

#[test]
fn retaining_messages_drops_the_rest_of_the_folder_only() {
    let mut store = Store::open_in_memory().unwrap();
    let inbox = FolderId("inbox".to_owned());
    let other = FolderId("other".to_owned());
    store
        .upsert_folders(&[
            folder("inbox", "Inbox", None),
            folder("other", "Other", None),
        ])
        .unwrap();
    let keep = message("m1", "inbox", "2026-01-01T09:00:00Z");
    let stale = message("m2", "inbox", "2026-01-02T09:00:00Z");
    let elsewhere = message("m3", "other", "2026-01-03T09:00:00Z");
    store
        .upsert_messages(&[keep.clone(), stale, elsewhere.clone()])
        .unwrap();

    store
        .retain_messages_in(&inbox, std::slice::from_ref(&keep.id))
        .unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [keep]);
    assert_eq!(store.messages_in(&other).unwrap(), [elsewhere]);
}

#[test]
fn a_fetched_body_is_cached_until_its_message_is_deleted() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    let m = message("m1", "inbox", "2026-01-01T09:00:00Z");
    store.upsert_messages(std::slice::from_ref(&m)).unwrap();
    let body = Body {
        content_type: BodyType::Html,
        content: "<p>Hi</p>".to_owned(),
    };
    assert_eq!(store.body(&m.id).unwrap(), None);

    store.put_body(&m.id, &body).unwrap();
    assert_eq!(store.body(&m.id).unwrap(), Some(body));

    store.delete_messages(std::slice::from_ref(&m.id)).unwrap();
    assert_eq!(store.body(&m.id).unwrap(), None);
}
