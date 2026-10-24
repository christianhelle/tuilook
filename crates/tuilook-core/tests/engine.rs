use tokio::sync::mpsc;
use tuilook_core::{Command, fake::FakeBackend, run_engine};
use tuilook_model::{
    BackendError, Body, BodyType, DeltaPage, Event, Folder, FolderId, Importance, Message,
    MessageId,
};
use tuilook_store::Store;

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
    events.recv().await;
    events.recv().await;

    commands.send(Command::LoadBody(id.clone())).await.unwrap();

    assert_eq!(
        events.recv().await,
        Some(Event::BodyLoaded {
            message: id,
            text: "Hi".to_owned()
        })
    );
}
