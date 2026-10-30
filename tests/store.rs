use tuilook::model::{
    Appointment, AppointmentId, Body, BodyType, Folder, FolderId, Importance, Message,
    MessageChange, MessageId, WellKnown,
};
use tuilook::store::Store;

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
        pinned: false,
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
fn a_change_updates_only_the_fields_it_carries() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .upsert_folders(&[folder("inbox", "Inbox", None)])
        .unwrap();
    let m = message("m1", "inbox", "2026-01-01T09:00:00Z");
    store.upsert_messages(std::slice::from_ref(&m)).unwrap();

    store
        .change_messages(&[
            MessageChange {
                is_read: Some(false),
                flagged: Some(true),
                ..MessageChange::new(m.id.clone())
            },
            // A change to a message that is not cached is dropped.
            MessageChange {
                is_read: Some(true),
                ..MessageChange::new(MessageId("unknown".to_owned()))
            },
        ])
        .unwrap();

    assert_eq!(
        store.messages_in(&FolderId("inbox".to_owned())).unwrap(),
        [Message {
            is_read: false,
            flagged: true,
            pinned: false,
            ..m
        }]
    );
}

#[test]
fn opening_a_cache_blanked_by_partial_changes_drops_them_and_resyncs() {
    // tuilook 0.1.0 stored a partial delta change as a whole message, so the
    // message lost its subject, sender and date (stored as 0).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let inbox = FolderId("inbox".to_owned());
    let projects = FolderId("projects".to_owned());
    let kept = message("m1", "inbox", "2026-01-01T09:00:00Z");
    {
        let mut store = Store::open(&path).unwrap();
        store
            .upsert_folders(&[
                folder("inbox", "Inbox", None),
                folder("projects", "Projects", None),
            ])
            .unwrap();
        let blank = Message {
            subject: String::new(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: jiff::Timestamp::UNIX_EPOCH,
            ..message("m2", "inbox", "2026-01-01T09:00:00Z")
        };
        store.upsert_messages(&[kept.clone(), blank]).unwrap();
        store.set_delta_link(&inbox, Some("inbox-link")).unwrap();
        store
            .set_delta_link(&projects, Some("projects-link"))
            .unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    // Nor did 0.1.0 have pins.
    conn.execute("ALTER TABLE messages DROP COLUMN pinned", [])
        .unwrap();
    // Or a calendar.
    conn.execute("DROP TABLE appointments", []).unwrap();
    drop(conn);

    let store = Store::open(&path).unwrap();

    assert_eq!(store.messages_in(&inbox).unwrap(), [kept]);
    assert_eq!(store.delta_link(&inbox).unwrap(), None);
    assert_eq!(
        store.delta_link(&projects).unwrap().as_deref(),
        Some("projects-link")
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
fn marking_a_message_read_counts_it_out_of_its_folders_unread() {
    let mut store = Store::open_in_memory().unwrap();
    let inbox = FolderId("inbox".to_owned());
    let mut f = folder("inbox", "Inbox", None);
    f.unread = 2;
    store.upsert_folders(&[f]).unwrap();
    let mut m = message("m1", "inbox", "2026-01-01T09:00:00Z");
    m.is_read = false;
    store.upsert_messages(&[m]).unwrap();

    assert_eq!(
        store.mark_read(&MessageId("m1".to_owned())).unwrap(),
        Some(inbox.clone())
    );
    assert!(store.messages_in(&inbox).unwrap()[0].is_read);
    assert_eq!(store.folders().unwrap()[0].unread, 1);

    assert_eq!(
        store.mark_read(&MessageId("m1".to_owned())).unwrap(),
        None,
        "already read"
    );
    assert_eq!(store.folders().unwrap()[0].unread, 1);
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

#[test]
fn two_handles_on_one_file_see_each_others_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let mut writer = Store::open(&path).unwrap();
    let reader = Store::open(&path).unwrap();

    writer
        .upsert_folders(&[folder("f1", "Inbox", None)])
        .unwrap();

    assert_eq!(reader.folders().unwrap(), [folder("f1", "Inbox", None)]);
}

/// A store with an Inbox and an Archive, holding `messages`.
fn store_with(messages: &[Message]) -> Store {
    let mut store = Store::open_in_memory().unwrap();
    store
        .upsert_folders(&[
            folder("inbox", "Inbox", None),
            folder("archive", "Archive", None),
        ])
        .unwrap();
    store.upsert_messages(messages).unwrap();
    store
}

fn ids(messages: &[Message]) -> Vec<&str> {
    messages.iter().map(|m| m.id.0.as_str()).collect()
}

#[test]
fn search_finds_subject_sender_and_preview_in_every_folder_ignoring_case() {
    let mut by_subject = message("subject", "inbox", "2026-01-01T09:00:00Z");
    by_subject.subject = "Lunch on Friday?".to_owned();
    let mut by_name = message("name", "archive", "2026-01-02T09:00:00Z");
    by_name.from_name = "Lunch Club".to_owned();
    let mut by_addr = message("addr", "inbox", "2026-01-03T09:00:00Z");
    by_addr.from_addr = "lunch@example.com".to_owned();
    let mut by_preview = message("preview", "archive", "2026-01-04T09:00:00Z");
    by_preview.preview = "Shall we have LUNCH?".to_owned();
    let other = message("other", "inbox", "2026-01-05T09:00:00Z");
    let store = store_with(&[by_subject, by_name, by_addr, by_preview, other]);

    let found = store.search("lunch", 100).unwrap();

    assert_eq!(ids(&found), ["preview", "addr", "name", "subject"]);
}

#[test]
fn search_finds_words_in_fetched_bodies() {
    let mut store = store_with(&[
        message("m1", "inbox", "2026-01-01T09:00:00Z"),
        message("m2", "inbox", "2026-01-02T09:00:00Z"),
    ]);
    let body = Body {
        content_type: BodyType::Text,
        content: "The invoice is attached.".to_owned(),
    };
    store.put_body(&MessageId("m1".to_owned()), &body).unwrap();

    let found = store.search("Invoice", 100).unwrap();

    assert_eq!(ids(&found), ["m1"]);
}

#[test]
fn search_takes_wildcards_literally() {
    let mut percent = message("percent", "inbox", "2026-01-01T09:00:00Z");
    percent.subject = "50% off".to_owned();
    let plain = message("plain", "inbox", "2026-01-02T09:00:00Z");
    let store = store_with(&[percent, plain]);

    assert_eq!(ids(&store.search("%", 100).unwrap()), ["percent"]);
    assert!(store.search("_x_", 100).unwrap().is_empty());
}

#[test]
fn search_returns_at_most_the_limit_newest_first() {
    let store = store_with(&[
        message("m1", "inbox", "2026-01-01T09:00:00Z"),
        message("m2", "archive", "2026-01-02T09:00:00Z"),
        message("m3", "inbox", "2026-01-03T09:00:00Z"),
    ]);

    let found = store.search("ada", 2).unwrap();

    assert_eq!(ids(&found), ["m3", "m2"]);
}

#[test]
fn unread_in_lists_a_folders_unread_mail_and_those_kept() {
    let mut read = message("read", "inbox", "2026-01-01T09:00:00Z");
    read.is_read = true;
    let mut kept = message("kept", "inbox", "2026-01-02T09:00:00Z");
    kept.is_read = true;
    let mut unread = message("unread", "inbox", "2026-01-03T09:00:00Z");
    unread.is_read = false;
    let mut elsewhere = message("elsewhere", "archive", "2026-01-04T09:00:00Z");
    elsewhere.is_read = false;
    let store = store_with(&[read, kept.clone(), unread, elsewhere]);
    let inbox = FolderId("inbox".to_owned());

    assert_eq!(ids(&store.unread_in(&inbox, &[]).unwrap()), ["unread"]);
    assert_eq!(
        ids(&store
            .unread_in(&inbox, std::slice::from_ref(&kept.id))
            .unwrap()),
        ["unread", "kept"]
    );
}

fn pinned(mut message: Message) -> Message {
    message.pinned = true;
    message
}

#[test]
fn pinned_messages_come_first_and_stay_pinned_when_synced_again() {
    let old = pinned(message("m1", "inbox", "2026-01-01T09:00:00Z"));
    let new = message("m2", "inbox", "2026-02-01T09:00:00Z");
    let mut store = store_with(&[old.clone(), new.clone()]);

    // Delta sync does not know about pins, so it sends the message unpinned.
    store
        .upsert_messages(&[message("m1", "inbox", "2026-01-01T09:00:00Z")])
        .unwrap();

    let inbox = FolderId("inbox".to_owned());
    assert_eq!(store.messages_in(&inbox).unwrap(), [old.clone(), new]);
    assert_eq!(ids(&store.search("subject", 10).unwrap()), ["m1", "m2"]);
}

#[test]
fn pinning_a_message_tells_its_folder_and_unpinning_takes_it_back() {
    let mut store = store_with(&[message("m1", "inbox", "2026-01-01T09:00:00Z")]);
    let id = MessageId("m1".to_owned());
    let inbox = FolderId("inbox".to_owned());

    assert_eq!(store.set_pinned(&id, true).unwrap(), Some(inbox.clone()));
    assert!(store.messages_in(&inbox).unwrap()[0].pinned);

    assert_eq!(store.set_pinned(&id, false).unwrap(), Some(inbox.clone()));
    assert!(!store.messages_in(&inbox).unwrap()[0].pinned);

    let missing = MessageId("gone".to_owned());
    assert_eq!(store.set_pinned(&missing, true).unwrap(), None);
}

#[test]
fn a_folders_pins_are_exactly_those_synced() {
    let mut store = store_with(&[
        pinned(message("m1", "inbox", "2026-01-01T09:00:00Z")),
        message("m2", "inbox", "2026-02-01T09:00:00Z"),
        pinned(message("m3", "archive", "2026-03-01T09:00:00Z")),
    ]);
    let inbox = FolderId("inbox".to_owned());

    store
        .set_pinned_in(&inbox, &[MessageId("m2".to_owned())])
        .unwrap();

    let pins = |folder: &str| -> Vec<String> {
        let folder = FolderId(folder.to_owned());
        let messages = store.messages_in(&folder).unwrap();
        messages
            .into_iter()
            .filter(|m| m.pinned)
            .map(|m| m.id.0)
            .collect()
    };
    assert_eq!(pins("inbox"), ["m2"]);
    assert_eq!(pins("archive"), ["m3"], "other folders keep their pins");
}

#[test]
fn flagging_a_message_tells_its_folder_and_unflagging_takes_it_back() {
    let mut store = store_with(&[message("m1", "inbox", "2026-01-01T09:00:00Z")]);
    let id = MessageId("m1".to_owned());
    let inbox = FolderId("inbox".to_owned());

    assert_eq!(store.set_flagged(&id, true).unwrap(), Some(inbox.clone()));
    assert!(store.messages_in(&inbox).unwrap()[0].flagged);

    assert_eq!(store.set_flagged(&id, false).unwrap(), Some(inbox.clone()));
    assert!(!store.messages_in(&inbox).unwrap()[0].flagged);

    let missing = MessageId("gone".to_owned());
    assert_eq!(store.set_flagged(&missing, true).unwrap(), None);
}

fn appointment(id: &str, start: &str, end: &str) -> Appointment {
    Appointment {
        id: AppointmentId(id.to_owned()),
        subject: format!("Subject {id}"),
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
        all_day: false,
        location: "Room 4".to_owned(),
        organizer: "Ada Lovelace".to_owned(),
        attendees: vec![
            "grace@example.com".to_owned(),
            "alan@example.com".to_owned(),
        ],
        preview: "Agenda inside".to_owned(),
        is_organizer: false,
    }
}

fn at(time: &str) -> jiff::Timestamp {
    time.parse().unwrap()
}

#[test]
fn cached_appointments_come_back_soonest_first() {
    let mut store = Store::open_in_memory().unwrap();
    let later = appointment("a2", "2026-11-03T09:00:00Z", "2026-11-03T10:00:00Z");
    let mut sooner = appointment("a1", "2026-11-02T09:00:00Z", "2026-11-02T10:00:00Z");
    sooner.all_day = true;
    sooner.is_organizer = true;

    store
        .replace_appointments(
            at("2026-11-01T00:00:00Z"),
            at("2026-12-01T00:00:00Z"),
            &[later.clone(), sooner.clone()],
        )
        .unwrap();

    assert_eq!(
        store
            .appointments_between(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
            .unwrap(),
        [sooner, later]
    );
}

#[test]
fn only_appointments_overlapping_the_window_come_back() {
    let mut store = Store::open_in_memory().unwrap();
    let before = appointment("a1", "2026-11-01T08:00:00Z", "2026-11-01T09:00:00Z");
    let overlapping = appointment("a2", "2026-11-01T23:30:00Z", "2026-11-02T00:30:00Z");
    store
        .replace_appointments(
            at("2026-10-01T00:00:00Z"),
            at("2026-12-01T00:00:00Z"),
            &[before, overlapping.clone()],
        )
        .unwrap();

    assert_eq!(
        store
            .appointments_between(at("2026-11-02T00:00:00Z"), at("2026-11-03T00:00:00Z"))
            .unwrap(),
        [overlapping]
    );
}

#[test]
fn replacing_a_window_drops_what_the_server_no_longer_has_there() {
    let mut store = Store::open_in_memory().unwrap();
    let outside = appointment("a1", "2026-09-01T09:00:00Z", "2026-09-01T10:00:00Z");
    let gone = appointment("a2", "2026-11-02T09:00:00Z", "2026-11-02T10:00:00Z");
    let mut moved = appointment("a3", "2026-11-03T09:00:00Z", "2026-11-03T10:00:00Z");
    store
        .replace_appointments(
            at("2026-08-01T00:00:00Z"),
            at("2026-12-01T00:00:00Z"),
            &[outside.clone(), gone, moved.clone()],
        )
        .unwrap();
    moved.start = at("2026-11-04T09:00:00Z");
    moved.end = at("2026-11-04T10:00:00Z");

    store
        .replace_appointments(
            at("2026-11-01T00:00:00Z"),
            at("2026-12-01T00:00:00Z"),
            &[moved.clone()],
        )
        .unwrap();

    assert_eq!(
        store
            .appointments_between(at("2026-08-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
            .unwrap(),
        [outside, moved]
    );
}

#[test]
fn a_removed_appointment_is_gone_from_the_cache() {
    let mut store = Store::open_in_memory().unwrap();
    let kept = appointment("a1", "2026-11-02T09:00:00Z", "2026-11-02T10:00:00Z");
    let removed = appointment("a2", "2026-11-03T09:00:00Z", "2026-11-03T10:00:00Z");
    store
        .replace_appointments(
            at("2026-11-01T00:00:00Z"),
            at("2026-12-01T00:00:00Z"),
            &[kept.clone(), removed.clone()],
        )
        .unwrap();

    store.remove_appointment(&removed.id).unwrap();

    assert_eq!(
        store
            .appointments_between(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
            .unwrap(),
        [kept]
    );
}
