use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};
use tokio::sync::mpsc;
use tuilook::core::{Command, calendar_window, fake::FakeBackend, run_engine};
use tuilook::model::{
    Appointment, AppointmentDraft, AppointmentId, Attachment, AttachmentId, AttachmentKind,
    BackendError, Body, BodyType, ComposeKind, DeltaPage, Draft, Event, Folder, FolderId,
    Importance, Invite, Message, MessageId, Rsvp, WellKnown,
};
use tuilook::store::Store;

fn folder(id: &str) -> Folder {
    tuilook::core::fake::folder(id)
}

fn start(backend: FakeBackend) -> (mpsc::UnboundedSender<Command>, mpsc::Receiver<Event>) {
    let (commands, command_rx) = mpsc::unbounded_channel();
    let (event_tx, events) = mpsc::channel(8);
    let store = Store::open_in_memory().unwrap();
    tokio::spawn(run_engine(backend, store, command_rx, event_tx));
    (commands, events)
}

#[tokio::test]
async fn sync_all_reports_the_folder_tree_then_each_folder() {
    let backend = FakeBackend {
        folders: vec![folder("a"), folder("b")],
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands.send(Command::SyncAll).unwrap();

    assert_eq!(events.recv().await, Some(Event::FoldersSynced));
    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("a".to_owned())))
    );
    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("b".to_owned())))
    );
    assert_eq!(events.recv().await, Some(Event::SyncFinished));
}

#[tokio::test]
async fn a_sync_that_cannot_reach_the_folders_still_finishes() {
    let backend = FakeBackend {
        folders_error: Some(BackendError::Other("offline".to_owned())),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands.send(Command::SyncAll).unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error("offline".to_owned()))
    );
    assert_eq!(events.recv().await, Some(Event::SyncFinished));
}

#[tokio::test]
async fn a_failing_folder_is_reported_and_the_rest_still_sync() {
    let backend = FakeBackend {
        folders: vec![folder("a"), folder("b")],
        deltas: [(
            (FolderId("a".to_owned()), None),
            Err::<DeltaPage, _>(BackendError::Other("boom".to_owned())),
        )]
        .into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands.send(Command::SyncAll).unwrap();

    assert_eq!(events.recv().await, Some(Event::FoldersSynced));
    assert_eq!(events.recv().await, Some(Event::Error("boom".to_owned())));
    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("b".to_owned())))
    );
}

#[tokio::test]
async fn load_body_reports_the_body_text() {
    let id = MessageId("m1".to_owned());
    let message = Message {
        id: id.clone(),
        folder_id: FolderId("a".to_owned()),
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
    };
    let backend = FakeBackend {
        folders: vec![folder("a")],
        messages: [(FolderId("a".to_owned()), vec![message])].into(),
        bodies: [(
            id.clone(),
            Body {
                content_type: BodyType::Text,
                content: "Hi".to_owned(),
            },
        )]
        .into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(Command::LoadBody(id.clone())).unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::BodyLoaded {
            message: id,
            text: "Hi".to_owned(),
            attachments: vec![],
        })
    );
}

#[tokio::test]
async fn mark_read_tells_the_backend_and_reports_the_folder_changed() {
    let mut inbox = folder("a");
    inbox.unread = 1;
    let message = Message {
        folder_id: FolderId("a".to_owned()),
        ..tuilook::core::fake::message("m1")
    };
    let backend = FakeBackend {
        folders: vec![inbox],
        messages: [(FolderId("a".to_owned()), vec![message])].into(),
        ..Default::default()
    };
    let read = backend.read.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(Command::MarkRead(MessageId("m1".to_owned())))
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("a".to_owned())))
    );
    assert!(read.lock().unwrap().contains(&MessageId("m1".to_owned())));
}

fn ids(ids: &[&str]) -> Vec<MessageId> {
    ids.iter().map(|id| MessageId((*id).to_owned())).collect()
}

#[tokio::test]
async fn delete_asks_the_backend_and_reports_what_went() {
    let backend = FakeBackend::default();
    let deleted = backend.deleted.clone();
    let (commands, mut events) = start(backend);

    commands
        .send(Command::Delete {
            ids: ids(&["m1", "m2"]),
            permanent: true,
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::MessagesDeleted(ids(&["m1", "m2"])))
    );
    assert_eq!(
        *deleted.lock().unwrap(),
        [
            (MessageId("m1".to_owned()), true),
            (MessageId("m2".to_owned()), true)
        ]
    );
}

#[tokio::test]
async fn a_message_that_will_not_delete_is_reported_and_the_rest_still_go() {
    let backend = FakeBackend {
        undeletable: ids(&["m2"]).into_iter().collect(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands
        .send(Command::Delete {
            ids: ids(&["m1", "m2"]),
            permanent: false,
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::MessagesDeleted(ids(&["m1"])))
    );
    assert_eq!(
        events.recv().await,
        Some(Event::Error("deleted 1, 1 failed: m2 is locked".to_owned()))
    );
}

fn draft(to: &str) -> Draft {
    Draft {
        kind: ComposeKind::New,
        to: vec![to.to_owned()],
        cc: vec![],
        subject: "Hello".to_owned(),
        body: "Hi".to_owned(),
        attachments: vec![],
    }
}

#[tokio::test]
async fn sending_reports_it_went_then_syncs_sent_items() {
    let mut sent_items = folder("sent");
    sent_items.well_known = Some(WellKnown::SentItems);
    let backend = FakeBackend {
        folders: vec![folder("inbox"), sent_items],
        ..Default::default()
    };
    let sent = backend.sent.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    for _ in 0..4 {
        events.recv().await;
    }

    commands
        .send(Command::Send(draft("ada@example.com")))
        .unwrap();

    assert_eq!(events.recv().await, Some(Event::Sent));
    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("sent".to_owned())))
    );
    assert_eq!(*sent.lock().unwrap(), [draft("ada@example.com")]);
}

#[tokio::test]
async fn a_message_that_cannot_be_sent_says_why() {
    let (commands, mut events) = start(FakeBackend::default());

    commands
        .send(Command::Send(draft("nobody@invalid")))
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::SendFailed("no such recipient".to_owned()))
    );
}

#[tokio::test]
async fn a_blocked_account_is_reported_as_blocked() {
    let (commands, mut events) = start(FakeBackend::default());

    commands.send(Command::Send(draft("me@blocked"))).unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::SendBlocked("spam suspected".to_owned()))
    );
}

fn old(id: &str, day: i64) -> Message {
    Message {
        folder_id: FolderId("a".to_owned()),
        received_at: jiff::Timestamp::from_second(day * 86_400).unwrap(),
        ..tuilook::core::fake::message(id)
    }
}

fn load_older(before: jiff::Timestamp) -> Command {
    Command::LoadOlder {
        folder: FolderId("a".to_owned()),
        before,
        unread_only: false,
    }
}

fn older_loaded(more: bool) -> Option<Event> {
    Some(Event::OlderLoaded {
        folder: FolderId("a".to_owned()),
        unread_only: false,
        more,
    })
}

#[tokio::test]
async fn older_mail_comes_a_page_at_a_time_until_there_is_no_more() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        older: [(
            FolderId("a".to_owned()),
            vec![old("m3", 3), old("m2", 2), old("m1", 1)],
        )]
        .into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(load_older(jiff::Timestamp::MAX)).unwrap();
    assert_eq!(events.recv().await, older_loaded(true));

    let day_2 = jiff::Timestamp::from_second(2 * 86_400).unwrap();
    commands.send(load_older(day_2)).unwrap();
    assert_eq!(events.recv().await, older_loaded(false));
}

#[tokio::test]
async fn a_page_of_older_mail_already_cached_says_there_is_no_more() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        older: [(
            FolderId("a".to_owned()),
            vec![old("m4", 4), old("m3", 3), old("m2", 2), old("m1", 1)],
        )]
        .into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(load_older(jiff::Timestamp::MAX)).unwrap();
    assert_eq!(events.recv().await, older_loaded(true));
    commands.send(load_older(jiff::Timestamp::MAX)).unwrap();
    assert_eq!(events.recv().await, older_loaded(false));
}

#[tokio::test]
async fn deleting_a_folder_reports_it_then_syncs_the_folder_tree() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        ..Default::default()
    };
    let deleted = backend.deleted_folders.clone();
    let (commands, mut events) = start(backend);

    commands
        .send(Command::DeleteFolder {
            folder: FolderId("a".to_owned()),
            permanent: true,
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::FolderDeleted(FolderId("a".to_owned())))
    );
    assert_eq!(events.recv().await, Some(Event::FoldersSynced));
    assert_eq!(*deleted.lock().unwrap(), [(FolderId("a".to_owned()), true)]);
}

#[tokio::test]
async fn creating_a_folder_reports_it_then_syncs_the_folder_tree() {
    let backend = FakeBackend {
        folders: vec![folder("inbox")],
        ..Default::default()
    };
    let created = backend.created.clone();
    let (commands, mut events) = start(backend);

    commands
        .send(Command::CreateFolder {
            parent: Some(FolderId("inbox".to_owned())),
            name: "Receipts".to_owned(),
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::FolderCreated("Receipts".to_owned()))
    );
    assert_eq!(events.recv().await, Some(Event::FoldersSynced));
    let created = created.lock().unwrap();
    assert_eq!(created[0].name, "Receipts");
    assert_eq!(created[0].parent_id, Some(FolderId("inbox".to_owned())));
}

#[tokio::test]
async fn a_folder_that_cannot_be_created_says_why() {
    let backend = FakeBackend {
        folders: vec![folder("inbox")],
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands
        .send(Command::CreateFolder {
            parent: None,
            name: "INBOX".to_owned(),
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not create the folder: INBOX already exists".to_owned()
        ))
    );
}

#[tokio::test]
async fn renaming_a_folder_reports_it_then_syncs_the_folder_tree() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        ..Default::default()
    };
    let renamed = backend.renamed.clone();
    let (commands, mut events) = start(backend);

    commands
        .send(Command::RenameFolder {
            folder: FolderId("a".to_owned()),
            name: "Bills".to_owned(),
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::FolderRenamed {
            folder: FolderId("a".to_owned()),
            name: "Bills".to_owned()
        })
    );
    assert_eq!(events.recv().await, Some(Event::FoldersSynced));
    assert_eq!(renamed.lock().unwrap()[&FolderId("a".to_owned())], "Bills");
}

fn in_folder(id: &str, folder: &str) -> Message {
    Message {
        folder_id: FolderId(folder.to_owned()),
        ..old(id, 1)
    }
}

#[tokio::test]
async fn moving_reports_what_went_then_syncs_the_folder_it_went_to() {
    let backend = FakeBackend {
        folders: vec![folder("a"), folder("b")],
        messages: [(
            FolderId("a".to_owned()),
            vec![in_folder("m1", "a"), in_folder("m2", "a")],
        )]
        .into(),
        ..Default::default()
    };
    let moved = backend.moved.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(Command::Move {
            ids: ids(&["m1"]),
            to: FolderId("b".to_owned()),
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::MessagesMoved {
            ids: ids(&["m1"]),
            to: FolderId("b".to_owned())
        })
    );
    assert_eq!(
        events.recv().await,
        Some(Event::FolderSynced(FolderId("b".to_owned())))
    );
    assert_eq!(
        *moved.lock().unwrap(),
        [(MessageId("m1".to_owned()), FolderId("b".to_owned()))]
    );
}

#[tokio::test]
async fn a_message_that_will_not_move_is_reported_and_the_rest_still_go() {
    let backend = FakeBackend {
        folders: vec![folder("b")],
        undeletable: ids(&["m2"]).into_iter().collect(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands
        .send(Command::Move {
            ids: ids(&["m1", "m2"]),
            to: FolderId("b".to_owned()),
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::MessagesMoved {
            ids: ids(&["m1"]),
            to: FolderId("b".to_owned())
        })
    );
    assert_eq!(
        events.recv().await,
        Some(Event::Error("moved 1, 1 failed: m2 is locked".to_owned()))
    );
}

/// The next event, or `None` when core stays quiet.
async fn next(events: &mut mpsc::Receiver<Event>) -> Option<Event> {
    let wait = std::time::Duration::from_secs(5);
    tokio::time::timeout(wait, events.recv())
        .await
        .ok()
        .flatten()
}

/// A core whose folder `a` holds m1, which the server refuses to change.
async fn with_a_message_that_will_not_change()
-> (mpsc::UnboundedSender<Command>, mpsc::Receiver<Event>) {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        messages: [(FolderId("a".to_owned()), vec![in_folder("m1", "a")])].into(),
        unchangeable: [MessageId("m1".to_owned())].into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}
    (commands, events)
}

#[tokio::test]
async fn a_failed_pin_still_reports_the_folder_so_the_ui_shows_it_unpinned() {
    let (commands, mut events) = with_a_message_that_will_not_change().await;

    commands
        .send(Command::SetPinned {
            ids: ids(&["m1"]),
            pinned: true,
        })
        .unwrap();

    assert_eq!(
        next(&mut events).await,
        Some(Event::Error("could not pin: refused".to_owned()))
    );
    assert_eq!(
        next(&mut events).await,
        Some(Event::FolderSynced(FolderId("a".to_owned())))
    );
}

#[tokio::test]
async fn a_failed_flag_still_reports_the_folder_so_the_ui_shows_it_unflagged() {
    let (commands, mut events) = with_a_message_that_will_not_change().await;

    commands
        .send(Command::SetFlagged {
            ids: ids(&["m1"]),
            flagged: true,
        })
        .unwrap();

    assert_eq!(
        next(&mut events).await,
        Some(Event::Error("could not flag: refused".to_owned()))
    );
    assert_eq!(
        next(&mut events).await,
        Some(Event::FolderSynced(FolderId("a".to_owned())))
    );
}

#[tokio::test]
async fn a_failed_mark_read_still_reports_the_folder_so_the_ui_shows_it_unread() {
    let (commands, mut events) = with_a_message_that_will_not_change().await;

    commands
        .send(Command::MarkRead(MessageId("m1".to_owned())))
        .unwrap();

    assert_eq!(
        next(&mut events).await,
        Some(Event::Error("could not mark as read: refused".to_owned()))
    );
    assert_eq!(
        next(&mut events).await,
        Some(Event::FolderSynced(FolderId("a".to_owned())))
    );
}

#[tokio::test]
async fn pinning_tells_the_backend_and_reports_the_folder_changed() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        messages: [(FolderId("a".to_owned()), vec![in_folder("m1", "a")])].into(),
        ..Default::default()
    };
    let pinned = backend.pinned.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    let pin = |pinned| Command::SetPinned {
        ids: ids(&["m1"]),
        pinned,
    };
    commands.send(pin(true)).unwrap();

    let changed = Some(Event::FolderSynced(FolderId("a".to_owned())));
    assert_eq!(events.recv().await, changed);
    assert!(pinned.lock().unwrap().contains(&MessageId("m1".to_owned())));

    commands.send(pin(false)).unwrap();

    assert_eq!(events.recv().await, changed);
    assert!(pinned.lock().unwrap().is_empty());
}

#[tokio::test]
async fn flagging_tells_the_backend_and_reports_the_folder_changed() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        messages: [(FolderId("a".to_owned()), vec![in_folder("m1", "a")])].into(),
        ..Default::default()
    };
    let flagged = backend.flagged.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    let flag = |flagged| Command::SetFlagged {
        ids: ids(&["m1"]),
        flagged,
    };
    commands.send(flag(true)).unwrap();

    let changed = Some(Event::FolderSynced(FolderId("a".to_owned())));
    assert_eq!(events.recv().await, changed);
    assert!(
        flagged
            .lock()
            .unwrap()
            .contains(&MessageId("m1".to_owned()))
    );

    commands.send(flag(false)).unwrap();

    assert_eq!(events.recv().await, changed);
    assert!(flagged.lock().unwrap().is_empty());
}

/// Starts core on a cache at `path`, which the test can open to look in.
fn start_on(
    backend: FakeBackend,
    path: &std::path::Path,
) -> (mpsc::UnboundedSender<Command>, mpsc::Receiver<Event>) {
    let (commands, command_rx) = mpsc::unbounded_channel();
    let (event_tx, events) = mpsc::channel(8);
    let store = Store::open(path).unwrap();
    tokio::spawn(run_engine(backend, store, command_rx, event_tx));
    (commands, events)
}

fn appointment(id: &str, start: Timestamp) -> Appointment {
    Appointment {
        id: AppointmentId(id.to_owned()),
        subject: format!("Subject {id}"),
        start,
        end: start + SignedDuration::from_hours(1),
        all_day: false,
        location: String::new(),
        organizer: String::new(),
        attendees: vec![],
        preview: String::new(),
        is_organizer: true,
    }
}

/// Now, to the second, as Graph gives times.
fn now() -> Timestamp {
    Timestamp::now().round(jiff::Unit::Second).unwrap()
}

fn cached_appointments(path: &std::path::Path) -> Vec<Appointment> {
    let (from, to) = calendar_window(Timestamp::now());
    Store::open(path)
        .unwrap()
        .appointments_between(from, to)
        .unwrap()
}

#[tokio::test]
async fn syncing_the_calendar_caches_the_appointments_around_today() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let tomorrow = appointment("a1", now() + SignedDuration::from_hours(24));
    let next_year = appointment(
        "a2",
        Timestamp::now() + SignedDuration::from_hours(400 * 24),
    );
    let backend = FakeBackend {
        appointments: Arc::new(Mutex::new(vec![tomorrow.clone(), next_year])),
        ..Default::default()
    };
    let (commands, mut events) = start_on(backend, &path);

    commands.send(Command::SyncCalendar).unwrap();

    assert_eq!(events.recv().await, Some(Event::CalendarSynced));
    assert_eq!(cached_appointments(&path), [tomorrow]);
}

#[tokio::test]
async fn a_calendar_that_cannot_sync_says_why() {
    let backend = FakeBackend {
        calendar_error: Some(BackendError::Other("offline".to_owned())),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands.send(Command::SyncCalendar).unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not sync the calendar: offline".to_owned()
        ))
    );
}

fn appointment_draft(subject: &str, start: Timestamp) -> AppointmentDraft {
    AppointmentDraft {
        subject: subject.to_owned(),
        start,
        end: start + SignedDuration::from_hours(1),
        all_day: false,
        location: String::new(),
        attendees: vec![],
        notes: None,
    }
}

#[tokio::test]
async fn a_saved_new_appointment_shows_once_the_calendar_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let (commands, mut events) = start_on(FakeBackend::default(), &path);
    let start = now() + SignedDuration::from_hours(2);

    commands
        .send(Command::CreateAppointment(appointment_draft(
            "Planning", start,
        )))
        .unwrap();

    assert_eq!(events.recv().await, Some(Event::AppointmentSaved));
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));
    let cached = cached_appointments(&path);
    assert_eq!(cached.len(), 1);
    assert_eq!(
        (cached[0].subject.as_str(), cached[0].start),
        ("Planning", start)
    );
}

#[tokio::test]
async fn a_changed_appointment_shows_changed_once_the_calendar_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let start = now() + SignedDuration::from_hours(2);
    let backend = FakeBackend {
        appointments: Arc::new(Mutex::new(vec![appointment("a1", start)])),
        ..Default::default()
    };
    let (commands, mut events) = start_on(backend, &path);

    commands
        .send(Command::UpdateAppointment {
            id: AppointmentId("a1".to_owned()),
            draft: appointment_draft("Moved", start + SignedDuration::from_hours(1)),
        })
        .unwrap();

    assert_eq!(events.recv().await, Some(Event::AppointmentSaved));
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));
    let cached = cached_appointments(&path);
    assert_eq!(
        (cached[0].subject.as_str(), cached[0].start),
        ("Moved", start + SignedDuration::from_hours(1))
    );
}

#[tokio::test]
async fn an_appointment_that_cannot_be_saved_says_why() {
    let backend = FakeBackend {
        calendar_error: Some(BackendError::Other("offline".to_owned())),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands
        .send(Command::CreateAppointment(appointment_draft(
            "Planning",
            now(),
        )))
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::AppointmentNotSaved("offline".to_owned()))
    );
}

#[tokio::test]
async fn a_deleted_appointment_leaves_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let kept = appointment("a1", now() + SignedDuration::from_hours(2));
    let deleted = appointment("a2", now() + SignedDuration::from_hours(4));
    let backend = FakeBackend {
        appointments: Arc::new(Mutex::new(vec![kept.clone(), deleted.clone()])),
        ..Default::default()
    };
    let (commands, mut events) = start_on(backend, &path);
    commands.send(Command::SyncCalendar).unwrap();
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));

    commands
        .send(Command::DeleteAppointment(deleted.id.clone()))
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::AppointmentDeleted(deleted.id))
    );
    assert_eq!(cached_appointments(&path), [kept]);
}

#[tokio::test]
async fn an_appointment_that_cannot_be_deleted_says_why() {
    let backend = FakeBackend {
        calendar_error: Some(BackendError::Other("offline".to_owned())),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    commands
        .send(Command::DeleteAppointment(AppointmentId("a1".to_owned())))
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not delete the appointment: offline".to_owned()
        ))
    );
}

fn attachment(id: &str, name: &str, kind: AttachmentKind) -> Attachment {
    Attachment {
        id: AttachmentId(id.to_owned()),
        name: name.to_owned(),
        content_type: String::new(),
        size: 3,
        kind,
    }
}

fn save(attachments: Vec<Attachment>, dir: &std::path::Path) -> Command {
    Command::SaveAttachments {
        message: MessageId("m1".to_owned()),
        attachments,
        dir: dir.to_path_buf(),
    }
}

#[tokio::test]
async fn saving_an_attachment_writes_it_beside_any_file_of_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("report.pdf"), "older").unwrap();
    let backend = FakeBackend {
        contents: [(AttachmentId("a1".to_owned()), b"pdf".to_vec())].into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    let report = attachment("a1", "report.pdf", AttachmentKind::File);
    commands.send(save(vec![report], dir.path())).unwrap();

    let saved = dir.path().join("report (1).pdf");
    assert_eq!(
        events.recv().await,
        Some(Event::AttachmentSaved(saved.clone()))
    );
    assert_eq!(std::fs::read(saved).unwrap(), b"pdf");
    assert_eq!(
        std::fs::read(dir.path().join("report.pdf")).unwrap(),
        b"older"
    );
}

#[tokio::test]
async fn saved_names_are_safe_attached_mail_is_eml_and_cloud_links_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend {
        contents: [
            (AttachmentId("a1".to_owned()), b"txt".to_vec()),
            (AttachmentId("a2".to_owned()), b"mime".to_vec()),
        ]
        .into(),
        ..Default::default()
    };
    let (commands, mut events) = start(backend);

    let attachments = vec![
        attachment("a1", "../plans: v2?.txt", AttachmentKind::File),
        attachment("a2", "Minutes", AttachmentKind::Item),
        attachment("a3", "Budget.xlsx", AttachmentKind::Reference),
    ];
    commands.send(save(attachments, dir.path())).unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::AttachmentSaved(dir.path().join(".._plans_ v2_.txt")))
    );
    assert_eq!(
        events.recv().await,
        Some(Event::AttachmentSaved(dir.path().join("Minutes.eml")))
    );
    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "Budget.xlsx is a link to a file in the cloud, with nothing to save".to_owned()
        ))
    );
}

fn invite() -> Invite {
    Invite {
        event: AppointmentId("e1".to_owned()),
        start: "2026-11-10T13:00:00Z".parse().unwrap(),
        end: "2026-11-10T14:00:00Z".parse().unwrap(),
        all_day: false,
        location: "Room 4".to_owned(),
        organizer: "Ada Lovelace".to_owned(),
        answer: None,
        response_requested: true,
    }
}

/// A backend with one message, `m1`, and `invites`, if it is one.
fn backend_with_mail(invites: Option<Invite>) -> (FakeBackend, MessageId) {
    let id = MessageId("m1".to_owned());
    let backend = FakeBackend {
        folders: vec![folder("inbox")],
        messages: [(
            FolderId("inbox".to_owned()),
            vec![tuilook::core::fake::message("m1")],
        )]
        .into(),
        bodies: [(
            id.clone(),
            Body {
                content_type: BodyType::Text,
                content: "Hi".to_owned(),
            },
        )]
        .into(),
        invites: Arc::new(Mutex::new(
            invites.into_iter().map(|i| (id.clone(), i)).collect(),
        )),
        ..Default::default()
    };
    (backend, id)
}

fn body_loaded(message: &MessageId) -> Event {
    Event::BodyLoaded {
        message: message.clone(),
        text: "Hi".to_owned(),
        attachments: vec![],
    }
}

#[tokio::test]
async fn opening_an_invite_loads_the_body_then_the_invite() {
    let (backend, id) = backend_with_mail(Some(invite()));
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(Command::LoadBody(id.clone())).unwrap();

    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(invite()),
        })
    );
}

#[tokio::test]
async fn a_message_known_not_to_be_an_invite_is_not_asked_about_again() {
    let (backend, id) = backend_with_mail(None);
    let lookups = backend.invite_lookups.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    // Answered only once the second open has finished with the invite.
    commands.send(Command::SyncCalendar).unwrap();
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));

    assert_eq!(*lookups.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_failed_invite_lookup_still_shows_the_body_and_tries_again_next_time() {
    let (backend, id) = backend_with_mail(Some(invite()));
    let failure = backend.invite_error.clone();
    *failure.lock().unwrap() = Some(BackendError::Other("offline".to_owned()));
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    // Let the first open finish before the server comes back.
    commands.send(Command::SyncCalendar).unwrap();
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));
    *failure.lock().unwrap() = None;
    commands.send(Command::LoadBody(id.clone())).unwrap();

    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(invite()),
        })
    );
}

#[tokio::test]
async fn a_known_invite_is_read_again_so_answers_given_elsewhere_show() {
    let (backend, id) = backend_with_mail(Some(invite()));
    let invites = backend.invites.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert!(matches!(
        events.recv().await,
        Some(Event::InviteLoaded { .. })
    ));

    let answered = Invite {
        answer: Some(Rsvp::Accept),
        ..invite()
    };
    invites.lock().unwrap().insert(id.clone(), answered.clone());
    commands.send(Command::LoadBody(id.clone())).unwrap();

    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(answered),
        })
    );
}

#[tokio::test]
async fn a_known_invite_that_cannot_be_read_again_shows_as_it_was() {
    let (backend, id) = backend_with_mail(Some(invite()));
    let failure = backend.invite_error.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert!(matches!(
        events.recv().await,
        Some(Event::InviteLoaded { .. })
    ));

    *failure.lock().unwrap() = Some(BackendError::Other("offline".to_owned()));
    commands.send(Command::LoadBody(id.clone())).unwrap();

    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(invite()),
        })
    );
}

#[tokio::test]
async fn answering_an_invite_tells_the_backend_remembers_it_and_syncs_the_calendar() {
    let (backend, id) = backend_with_mail(Some(invite()));
    let responses = backend.responses.clone();
    let failure = backend.invite_error.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert!(matches!(
        events.recv().await,
        Some(Event::InviteLoaded { .. })
    ));

    commands
        .send(Command::Respond {
            message: id.clone(),
            event: AppointmentId("e1".to_owned()),
            rsvp: Rsvp::Accept,
            notify: true,
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::InviteAnswered {
            message: id.clone(),
            answer: Rsvp::Accept,
        })
    );
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));
    assert_eq!(
        *responses.lock().unwrap(),
        [(AppointmentId("e1".to_owned()), Rsvp::Accept, true)]
    );

    // The answer is there to be served when the server cannot be asked.
    *failure.lock().unwrap() = Some(BackendError::Other("offline".to_owned()));
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    let answered = Invite {
        answer: Some(Rsvp::Accept),
        ..invite()
    };
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(answered),
        })
    );
}

#[tokio::test]
async fn a_refused_answer_to_an_invite_is_reported_and_changes_nothing() {
    let (mut backend, id) = backend_with_mail(Some(invite()));
    backend.calendar_error = Some(BackendError::Other("refused".to_owned()));
    let responses = backend.responses.clone();
    let failure = backend.invite_error.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert!(matches!(
        events.recv().await,
        Some(Event::InviteLoaded { .. })
    ));

    commands
        .send(Command::Respond {
            message: id.clone(),
            event: AppointmentId("e1".to_owned()),
            rsvp: Rsvp::Decline,
            notify: true,
        })
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not answer the invite: refused".to_owned()
        ))
    );
    assert!(responses.lock().unwrap().is_empty());
    *failure.lock().unwrap() = Some(BackendError::Other("offline".to_owned()));
    commands.send(Command::LoadBody(id.clone())).unwrap();
    assert_eq!(events.recv().await, Some(body_loaded(&id)));
    assert_eq!(
        events.recv().await,
        Some(Event::InviteLoaded {
            message: id,
            invite: Box::new(invite()),
        })
    );
}

#[tokio::test]
async fn the_demo_has_an_invite_that_stays_answered() {
    use tuilook::model::MailBackend;
    let (_, backend) = tuilook::tui::demo::accounts().remove(1);
    let found = backend.invites.lock().unwrap().clone();
    let (id, invite) = found.into_iter().next().unwrap();

    backend
        .respond(&invite.event, Rsvp::Accept, true)
        .await
        .unwrap();

    let answered = backend.invite(&id).await.unwrap().unwrap();
    assert_eq!(answered.answer, Some(Rsvp::Accept));
}
