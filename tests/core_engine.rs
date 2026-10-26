use tokio::sync::mpsc;
use tuilook::core::{Command, fake::FakeBackend, run_engine};
use tuilook::model::{
    BackendError, Body, BodyType, ComposeKind, DeltaPage, Draft, Event, Folder, FolderId,
    Importance, Message, MessageId, WellKnown,
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
