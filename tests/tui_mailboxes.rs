use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tuilook::core::{Command, calendar_window};
use tuilook::model::{
    Action, Appointment, AppointmentDraft, AppointmentId, Event, Folder, FolderId, Importance,
    Message, MessageId,
};
use tuilook::store::Store;
use tuilook::tui::{
    app::{AppState, Effect, Status},
    clipboard,
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
};

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

fn message(id: &str, subject: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("inbox".to_owned()),
        conversation_id: String::new(),
        subject: subject.to_owned(),
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

fn ids(ids: &[&str]) -> Vec<MessageId> {
    ids.iter().map(|id| MessageId((*id).to_owned())).collect()
}

/// What is written to the terminal's clipboard.
#[derive(Clone, Default)]
struct Clipboard(Rc<RefCell<Vec<u8>>>);

impl Write for Clipboard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Mailboxes whose caches each hold an inbox with `messages`, one list per
/// account, and the commands each core is sent.
struct Setup {
    app: AppState,
    mailboxes: Mailboxes,
    commands: Vec<mpsc::Receiver<Command>>,
    clipboard: Clipboard,
}

fn setup(accounts: &[&[Message]]) -> Setup {
    let mut app = AppState::default();
    let mut mailboxes = Vec::new();
    let mut commands = Vec::new();
    for (account, messages) in accounts.iter().enumerate() {
        let mut store = Store::open_in_memory().unwrap();
        store.upsert_folders(&[folder("inbox")]).unwrap();
        store.upsert_messages(messages).unwrap();
        let (tx, rx) = mpsc::channel(32);
        mailboxes.push(Mailbox {
            store,
            commands: tx,
        });
        commands.push(rx);
        app.accounts.push(format!("me{account}@outlook.com"));
    }
    let clipboard = Clipboard::default();
    let mailboxes = Mailboxes::new(mailboxes, None, Box::new(clipboard.clone()));
    Setup {
        app,
        mailboxes,
        commands,
        clipboard,
    }
}

/// Types `query` after `/`, performing each search it asks for.
fn search(app: &mut AppState, mailboxes: &mut Mailboxes, query: &str) {
    let mut keymap = Keymap::default();
    app.update(Action::Search);
    for c in query.chars() {
        let effect = app.on_key(KeyEvent::from(KeyCode::Char(c)), &mut keymap);
        mailboxes.apply(app, effect).unwrap();
    }
}

/// The deletes `commands` were sent, leaving out the bodies loaded.
fn deleted(commands: &mut mpsc::Receiver<Command>) -> Vec<Command> {
    sent(commands)
        .into_iter()
        .filter(|c| matches!(c, Command::Delete { .. }))
        .collect()
}

fn sent(commands: &mut mpsc::Receiver<Command>) -> Vec<Command> {
    std::iter::from_fn(|| commands.try_recv().ok()).collect()
}

#[test]
fn loading_a_folder_shows_its_messages_from_the_cache() {
    let mut s = setup(&[&[message("m1", "Hello")]]);

    s.mailboxes
        .apply(
            &mut s.app,
            Some(Effect::LoadFolder(FolderId("inbox".to_owned()))),
        )
        .unwrap();

    assert_eq!(s.app.messages, [message("m1", "Hello")]);
}

#[test]
fn a_sync_goes_to_every_account_and_its_calendar() {
    let mut s = setup(&[&[], &[]]);

    s.mailboxes.apply(&mut s.app, Some(Effect::Sync)).unwrap();

    for commands in &mut s.commands {
        assert_eq!(sent(commands), [Command::SyncAll, Command::SyncCalendar]);
    }
}

#[test]
fn deleting_search_results_asks_each_account_to_delete_its_own() {
    let mut s = setup(&[&[message("a1", "lunch")], &[message("b1", "lunch")]]);
    search(&mut s.app, &mut s.mailboxes, "lunch");
    assert_eq!(s.app.messages.len(), 2);

    let delete = Effect::Delete {
        ids: ids(&["a1", "b1"]),
        permanent: false,
    };
    s.mailboxes.apply(&mut s.app, Some(delete)).unwrap();

    let deletes = |id| {
        vec![Command::Delete {
            ids: ids(&[id]),
            permanent: false,
        }]
    };
    assert_eq!(deleted(&mut s.commands[0]), deletes("a1"));
    assert_eq!(deleted(&mut s.commands[1]), deletes("b1"));
}

#[test]
fn a_folder_synced_while_results_are_shown_searches_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let mut core = Store::open(&path).unwrap();
    core.upsert_folders(&[folder("inbox")]).unwrap();
    core.upsert_messages(&[message("m1", "lunch")]).unwrap();
    let (tx, _rx) = mpsc::channel(32);
    let mailbox = Mailbox {
        store: Store::open(&path).unwrap(),
        commands: tx,
    };
    let mut mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    let mut app = AppState::default();
    search(&mut app, &mut mailboxes, "lunch");

    // Core syncs a new match into the cache.
    core.upsert_messages(&[message("m2", "lunch again")])
        .unwrap();
    mailboxes
        .on_event(
            &mut app,
            0,
            Event::FolderSynced(FolderId("inbox".to_owned())),
        )
        .unwrap();

    assert_eq!(app.results_query(), Some("lunch"));
    assert_eq!(app.messages.len(), 2);
}

#[test]
fn copied_text_goes_to_the_terminal_clipboard() {
    let mut s = setup(&[&[]]);

    s.mailboxes
        .apply(&mut s.app, Some(Effect::Copy("hi".to_owned())))
        .unwrap();

    assert_eq!(*s.clipboard.0.borrow(), clipboard::osc52("hi").into_bytes());
}

#[test]
fn errors_say_which_account_they_are_from_when_there_are_several() {
    let mut s = setup(&[&[], &[]]);

    s.mailboxes
        .on_event(&mut s.app, 1, Event::Error("offline".to_owned()))
        .unwrap();

    assert_eq!(
        s.app.status,
        Some(Status::Error("me1@outlook.com: offline".to_owned()))
    );
}

fn appointment(id: &str, start: jiff::Timestamp) -> Appointment {
    Appointment {
        id: AppointmentId(id.to_owned()),
        subject: id.to_owned(),
        start,
        end: start + jiff::SignedDuration::from_hours(1),
        all_day: false,
        location: String::new(),
        organizer: String::new(),
        attendees: vec![],
        preview: String::new(),
        is_organizer: true,
    }
}

/// Puts `appointments` in the calendar cache at `path`, as core would.
fn cache_calendar(path: &std::path::Path, appointments: &[Appointment]) {
    let (from, to) = calendar_window(jiff::Timestamp::now());
    Store::open(path)
        .unwrap()
        .replace_appointments(from, to, appointments)
        .unwrap();
}

fn shift_c() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT)
}

#[test]
fn shift_c_opens_the_calendar_from_the_cache_and_syncs_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let soon = appointment(
        "soon",
        jiff::Timestamp::now().round(jiff::Unit::Second).unwrap(),
    );
    cache_calendar(&path, std::slice::from_ref(&soon));
    let (tx, mut rx) = mpsc::channel(32);
    let mailbox = Mailbox {
        store: Store::open(&path).unwrap(),
        commands: tx,
    };
    let mut mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    let mut app = AppState {
        accounts: vec!["me@outlook.com".to_owned()],
        ..Default::default()
    };

    let effect = app.on_key(shift_c(), &mut Keymap::default());
    mailboxes.apply(&mut app, effect).unwrap();

    assert_eq!(app.agenda.as_ref().unwrap().appointments(), [soon]);
    assert_eq!(sent(&mut rx), [Command::SyncCalendar]);
}

#[test]
fn a_synced_calendar_shows_in_the_open_agenda() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let (tx, _rx) = mpsc::channel(32);
    let mailbox = Mailbox {
        store: Store::open(&path).unwrap(),
        commands: tx,
    };
    let mut mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    let mut app = AppState::default();
    let effect = app.on_key(shift_c(), &mut Keymap::default());
    mailboxes.apply(&mut app, effect).unwrap();
    assert!(app.agenda.as_ref().unwrap().appointments().is_empty());

    let soon = appointment(
        "soon",
        jiff::Timestamp::now().round(jiff::Unit::Second).unwrap(),
    );
    cache_calendar(&path, std::slice::from_ref(&soon));
    mailboxes
        .on_event(&mut app, 0, Event::CalendarSynced)
        .unwrap();

    assert_eq!(app.agenda.as_ref().unwrap().appointments(), [soon]);
}

fn draft(subject: &str) -> AppointmentDraft {
    let start = jiff::Timestamp::UNIX_EPOCH;
    AppointmentDraft {
        subject: subject.to_owned(),
        start,
        end: start,
        all_day: false,
        location: String::new(),
        attendees: vec![],
        notes: None,
    }
}

#[test]
fn saving_or_deleting_an_appointment_goes_to_the_agendas_account() {
    let mut s = setup(&[&[], &[]]);
    let new = Effect::SaveAppointment {
        account: 1,
        id: None,
        draft: draft("New"),
    };
    let changed = Effect::SaveAppointment {
        account: 1,
        id: Some(AppointmentId("a1".to_owned())),
        draft: draft("Changed"),
    };
    let deleted = Effect::DeleteAppointment {
        account: 1,
        id: AppointmentId("a2".to_owned()),
    };

    for effect in [new, changed, deleted] {
        s.mailboxes.apply(&mut s.app, Some(effect)).unwrap();
    }

    assert!(sent(&mut s.commands[0]).is_empty());
    assert_eq!(
        sent(&mut s.commands[1]),
        [
            Command::CreateAppointment(draft("New")),
            Command::UpdateAppointment {
                id: AppointmentId("a1".to_owned()),
                draft: draft("Changed"),
            },
            Command::DeleteAppointment(AppointmentId("a2".to_owned())),
        ]
    );
}

#[test]
fn an_error_lets_the_appointment_being_saved_be_saved_again() {
    let mut s = setup(&[&[]]);
    let effect = s.app.on_key(shift_c(), &mut Keymap::default());
    s.mailboxes.apply(&mut s.app, effect).unwrap();
    s.app
        .on_key(KeyEvent::from(KeyCode::Char('c')), &mut Keymap::default());
    for c in "Planning".chars() {
        s.app
            .on_key(KeyEvent::from(KeyCode::Char(c)), &mut Keymap::default());
    }
    let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let effect = s.app.on_key(ctrl_s, &mut Keymap::default());
    s.mailboxes.apply(&mut s.app, effect).unwrap();
    assert!(s.app.appointment.as_ref().unwrap().saving);

    s.mailboxes
        .on_event(
            &mut s.app,
            0,
            Event::Error("could not save the appointment: offline".to_owned()),
        )
        .unwrap();

    assert!(!s.app.appointment.as_ref().unwrap().saving);
    assert_eq!(
        s.app.status,
        Some(Status::Error(
            "could not save the appointment: offline".to_owned()
        ))
    );
}
