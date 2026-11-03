use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use tokio::{sync::mpsc, time::timeout};
use tuilook::core::{Command, fake, fake::FakeBackend, run_engine};
use tuilook::model::{AppointmentId, Body, BodyType, Event, FolderId, Invite, MessageId, Rsvp};
use tuilook::store::Store;
use tuilook::tui::{
    app::{AppState, Status},
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
};

/// Passes what core says on to the app, until `done` says that is enough.
async fn pump(
    events: &mut mpsc::Receiver<Event>,
    mailboxes: &mut Mailboxes,
    app: &mut AppState,
    done: impl Fn(&AppState) -> bool,
) {
    while !done(app) {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("core went quiet")
            .expect("core stopped");
        mailboxes.on_event(app, 0, event).unwrap();
    }
}

#[tokio::test]
async fn an_invite_is_shown_when_opened_and_answered_with_two_keys() {
    let id = MessageId("m1".to_owned());
    let invite = Invite {
        event: AppointmentId("e1".to_owned()),
        start: "2026-11-10T13:00:00Z".parse().unwrap(),
        end: "2026-11-10T14:00:00Z".parse().unwrap(),
        all_day: false,
        location: "Room 4".to_owned(),
        organizer: "Ada Lovelace".to_owned(),
        answer: None,
        response_requested: true,
    };
    let mail = fake::message("m1");
    let backend = FakeBackend {
        folders: vec![fake::folder("inbox")],
        messages: [(FolderId("inbox".to_owned()), vec![mail.clone()])].into(),
        bodies: [(
            id.clone(),
            Body {
                content_type: BodyType::Text,
                content: "Hi".to_owned(),
            },
        )]
        .into(),
        invites: std::sync::Arc::new(std::sync::Mutex::new([(id.clone(), invite)].into())),
        ..Default::default()
    };
    let responses = backend.responses.clone();
    let (commands, command_rx) = mpsc::unbounded_channel();
    let (event_tx, mut events) = mpsc::channel(8);
    let core_store = Store::open_in_memory().unwrap();
    tokio::spawn(run_engine(backend, core_store, command_rx, event_tx));
    commands.send(Command::SyncAll).unwrap();
    while events.recv().await != Some(Event::SyncFinished) {}

    let store = Store::open_in_memory().unwrap();
    let mut mailboxes = Mailboxes::new(
        vec![Mailbox { store, commands }],
        None,
        Box::new(std::io::sink()),
    );
    let mut app = AppState::default();
    app.accounts.push("me@outlook.com".to_owned());
    let mut keymap = Keymap::default();

    let opening = app.set_messages(vec![mail]);
    mailboxes.apply(&mut app, opening).unwrap();
    pump(&mut events, &mut mailboxes, &mut app, |app| {
        app.invite.is_some()
    })
    .await;
    let text = app.reader_text().unwrap().join("\n");
    assert!(text.contains("Answer:   Not answered"), "{text}");

    for c in ['i', 'a'] {
        let press = KeyEvent::from(KeyCode::Char(c));
        let effect = app.on_key(press, &mut keymap);
        mailboxes.apply(&mut app, effect).unwrap();
    }
    pump(&mut events, &mut mailboxes, &mut app, |app| {
        matches!(app.status, Some(Status::Info(_)))
    })
    .await;

    assert_eq!(
        *responses.lock().unwrap(),
        [(AppointmentId("e1".to_owned()), Rsvp::Accept, true)]
    );
    let text = app.reader_text().unwrap().join("\n");
    assert!(text.contains("Answer:   Accepted"), "{text}");
}
