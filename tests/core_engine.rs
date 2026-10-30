use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};
use tokio::sync::mpsc;
use tuilook::core::{Command, calendar_window, fake::FakeBackend, run_engine};
use tuilook::model::{
    Appointment, AppointmentDraft, AppointmentId, BackendError, Body, BodyType, ComposeKind,
    DeltaPage, Draft, Event, Folder, FolderId, Importance, Message, MessageId, WellKnown,
};
use tuilook::store::Store;

fn folder(id: &str) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: None,
        name: id.to_owned(),
        well_known: None,
        unread: 0,
        total: 0,
    }
}

fn start(backend: FakeBackend) -> (mpsc::Sender<Command>, mpsc::Receiver<Event>) {
    let (commands, command_rx) = mpsc::channel(8);
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

    commands.send(Command::SyncAll).await.unwrap();

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

    commands.send(Command::SyncAll).await.unwrap();

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

    commands.send(Command::SyncAll).await.unwrap();

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
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands.send(Command::LoadBody(id.clone())).await.unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::BodyLoaded {
            message: id,
            text: "Hi".to_owned()
        })
    );
}

#[tokio::test]
async fn mark_read_tells_the_backend_and_reports_the_folder_changed() {
    let mut inbox = folder("a");
    inbox.unread = 1;
    let message = Message {
        id: MessageId("m1".to_owned()),
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
        folders: vec![inbox],
        messages: [(FolderId("a".to_owned()), vec![message])].into(),
        ..Default::default()
    };
    let read = backend.read.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(Command::MarkRead(MessageId("m1".to_owned())))
        .await
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
        .await
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
        .await
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
    commands.send(Command::SyncAll).await.unwrap();
    for _ in 0..4 {
        events.recv().await;
    }

    commands
        .send(Command::Send(draft("ada@example.com")))
        .await
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
        .await
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::SendFailed("no such recipient".to_owned()))
    );
}

#[tokio::test]
async fn a_blocked_account_is_reported_as_blocked() {
    let (commands, mut events) = start(FakeBackend::default());

    commands
        .send(Command::Send(draft("me@blocked")))
        .await
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::SendBlocked("spam suspected".to_owned()))
    );
}

fn old(id: &str, day: i64) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("a".to_owned()),
        conversation_id: String::new(),
        subject: String::new(),
        from_name: String::new(),
        from_addr: String::new(),
        received_at: jiff::Timestamp::from_second(day * 86_400).unwrap(),
        is_read: false,
        flagged: false,
        pinned: false,
        importance: Importance::Normal,
        has_attachments: false,
        preview: String::new(),
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
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(load_older(jiff::Timestamp::MAX))
        .await
        .unwrap();
    assert_eq!(events.recv().await, older_loaded(true));

    let day_2 = jiff::Timestamp::from_second(2 * 86_400).unwrap();
    commands.send(load_older(day_2)).await.unwrap();
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
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(load_older(jiff::Timestamp::MAX))
        .await
        .unwrap();
    assert_eq!(events.recv().await, older_loaded(true));
    commands
        .send(load_older(jiff::Timestamp::MAX))
        .await
        .unwrap();
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
        .await
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
        .await
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
        .await
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
        .await
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
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    commands
        .send(Command::Move {
            ids: ids(&["m1"]),
            to: FolderId("b".to_owned()),
        })
        .await
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
        .await
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

#[tokio::test]
async fn pinning_tells_the_backend_and_reports_the_folder_changed() {
    let backend = FakeBackend {
        folders: vec![folder("a")],
        messages: [(FolderId("a".to_owned()), vec![in_folder("m1", "a")])].into(),
        ..Default::default()
    };
    let pinned = backend.pinned.clone();
    let (commands, mut events) = start(backend);
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    let pin = |pinned| Command::SetPinned {
        ids: ids(&["m1"]),
        pinned,
    };
    commands.send(pin(true)).await.unwrap();

    let changed = Some(Event::FolderSynced(FolderId("a".to_owned())));
    assert_eq!(events.recv().await, changed);
    assert!(pinned.lock().unwrap().contains(&MessageId("m1".to_owned())));

    commands.send(pin(false)).await.unwrap();

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
    commands.send(Command::SyncAll).await.unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    let flag = |flagged| Command::SetFlagged {
        ids: ids(&["m1"]),
        flagged,
    };
    commands.send(flag(true)).await.unwrap();

    let changed = Some(Event::FolderSynced(FolderId("a".to_owned())));
    assert_eq!(events.recv().await, changed);
    assert!(
        flagged
            .lock()
            .unwrap()
            .contains(&MessageId("m1".to_owned()))
    );

    commands.send(flag(false)).await.unwrap();

    assert_eq!(events.recv().await, changed);
    assert!(flagged.lock().unwrap().is_empty());
}

/// Starts core on a cache at `path`, which the test can open to look in.
fn start_on(
    backend: FakeBackend,
    path: &std::path::Path,
) -> (mpsc::Sender<Command>, mpsc::Receiver<Event>) {
    let (commands, command_rx) = mpsc::channel(8);
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

    commands.send(Command::SyncCalendar).await.unwrap();

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

    commands.send(Command::SyncCalendar).await.unwrap();

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
        notes: String::new(),
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
        .await
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
        .await
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
        .await
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not save the appointment: offline".to_owned()
        ))
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
    commands.send(Command::SyncCalendar).await.unwrap();
    assert_eq!(events.recv().await, Some(Event::CalendarSynced));

    commands
        .send(Command::DeleteAppointment(deleted.id.clone()))
        .await
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
        .await
        .unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::Error(
            "could not delete the appointment: offline".to_owned()
        ))
    );
}
