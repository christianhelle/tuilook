use std::{
    cell::RefCell,
    io::{self, Write},
    rc::Rc,
};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tuilook::core::{Command, calendar_window};
use tuilook::model::{
    Action, Appointment, AppointmentDraft, AppointmentId, Event, Folder, FolderId, Message,
    MessageId,
};
use tuilook::store::Store;
use tuilook::tui::{
    app::{AppState, Confirm, Effect, Status},
    clipboard,
    dialog::Dialog,
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
};

fn folder(id: &str) -> Folder {
    tuilook::core::fake::folder(id)
}

fn message(id: &str, subject: &str) -> Message {
    Message {
        subject: subject.to_owned(),
        ..tuilook::core::fake::message(id)
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
    commands: Vec<mpsc::UnboundedReceiver<Command>>,
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
        let (tx, rx) = mpsc::unbounded_channel();
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
fn deleted(commands: &mut mpsc::UnboundedReceiver<Command>) -> Vec<Command> {
    sent(commands)
        .into_iter()
        .filter(|c| matches!(c, Command::Delete { .. }))
        .collect()
}

fn sent(commands: &mut mpsc::UnboundedReceiver<Command>) -> Vec<Command> {
    std::iter::from_fn(|| commands.try_recv().ok()).collect()
}

#[test]
fn a_core_too_busy_to_take_commands_does_not_hold_up_the_ui() {
    let mut s = setup(&[&[message("m1", "Hello")]]);

    for _ in 0..100 {
        let load = Some(Effect::core(
            0,
            Command::LoadBody(MessageId("m1".to_owned())),
        ));
        s.mailboxes.apply(&mut s.app, load).unwrap();
    }

    assert_eq!(sent(&mut s.commands[0]).len(), 100);
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

    assert_eq!(s.app.mail.messages, [message("m1", "Hello")]);
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
    assert_eq!(s.app.mail.messages.len(), 2);

    s.app.dialog = Some(Dialog::Confirm(Confirm::Delete {
        ids: ids(&["a1", "b1"]),
        permanent: false,
    }));
    let delete = s
        .app
        .on_key(KeyEvent::from(KeyCode::Char('y')), &mut Keymap::default());
    s.mailboxes.apply(&mut s.app, delete).unwrap();

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
    let (tx, _rx) = mpsc::unbounded_channel();
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
    assert_eq!(app.mail.messages.len(), 2);
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
    let (tx, mut rx) = mpsc::unbounded_channel();
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

    assert_eq!(app.screen.agenda().unwrap().appointments(), [soon]);
    assert_eq!(sent(&mut rx), [Command::SyncCalendar]);
}

#[test]
fn a_month_beyond_the_cached_calendar_is_asked_for_and_stays_through_a_sync() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mailbox = Mailbox {
        store: Store::open(&path).unwrap(),
        commands: tx,
    };
    let mut mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    let mut app = AppState::default();
    let mut keymap = Keymap::default();
    let effect = app.on_key(shift_c(), &mut keymap);
    mailboxes.apply(&mut app, effect).unwrap();
    sent(&mut rx);
    let effect = app.on_key(KeyEvent::from(KeyCode::Char('m')), &mut keymap);
    mailboxes.apply(&mut app, effect).unwrap();
    sent(&mut rx);

    // Six months on is always past the cached three months.
    for _ in 0..6 {
        let effect = app.on_key(KeyEvent::from(KeyCode::PageDown), &mut keymap);
        mailboxes.apply(&mut app, effect).unwrap();
    }
    let asked = sent(&mut rx);
    assert!(
        matches!(asked.last(), Some(Command::SyncCalendarBetween { .. })),
        "{asked:?}"
    );

    let (first, _) = app.screen.agenda().unwrap().visible_days().unwrap();
    let start = first
        .to_zoned(app.screen.agenda().unwrap().time_zone().clone())
        .unwrap()
        .timestamp()
        + jiff::SignedDuration::from_hours(36);
    let far = appointment("far", start);
    {
        let mut store = Store::open(&path).unwrap();
        store
            .replace_appointments(
                start - jiff::SignedDuration::from_hours(1),
                start + jiff::SignedDuration::from_hours(2),
                std::slice::from_ref(&far),
            )
            .unwrap();
    }

    // As the sync every minute does.
    mailboxes
        .on_event(&mut app, 0, Event::CalendarSynced)
        .unwrap();

    assert_eq!(app.screen.agenda().unwrap().appointments(), [far]);
}

#[test]
fn a_synced_calendar_shows_in_the_open_agenda() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let (tx, _rx) = mpsc::unbounded_channel();
    let mailbox = Mailbox {
        store: Store::open(&path).unwrap(),
        commands: tx,
    };
    let mut mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    let mut app = AppState::default();
    let effect = app.on_key(shift_c(), &mut Keymap::default());
    mailboxes.apply(&mut app, effect).unwrap();
    assert!(app.screen.agenda().unwrap().appointments().is_empty());

    let soon = appointment(
        "soon",
        jiff::Timestamp::now().round(jiff::Unit::Second).unwrap(),
    );
    cache_calendar(&path, std::slice::from_ref(&soon));
    mailboxes
        .on_event(&mut app, 0, Event::CalendarSynced)
        .unwrap();

    assert_eq!(app.screen.agenda().unwrap().appointments(), [soon]);
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
    let new = Effect::core(1, Command::CreateAppointment(draft("New")));
    let changed = Effect::core(
        1,
        Command::UpdateAppointment {
            id: AppointmentId("a1".to_owned()),
            draft: draft("Changed"),
        },
    );
    let deleted = Effect::core(
        1,
        Command::DeleteAppointment(AppointmentId("a2".to_owned())),
    );

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
fn an_unrelated_error_leaves_the_appointment_saving() {
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

    s.mailboxes
        .on_event(&mut s.app, 0, Event::Error("a sync failed".to_owned()))
        .unwrap();

    assert!(s.app.screen.appointment().unwrap().saving);
}

#[test]
fn a_failed_save_lets_the_appointment_be_saved_again() {
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
    assert!(s.app.screen.appointment().unwrap().saving);

    s.mailboxes
        .on_event(
            &mut s.app,
            0,
            Event::AppointmentNotSaved("offline".to_owned()),
        )
        .unwrap();

    assert!(!s.app.screen.appointment().unwrap().saving);
    assert_eq!(
        s.app.status,
        Some(Status::Error(
            "could not save the appointment: offline".to_owned()
        ))
    );
}

#[test]
fn tab_in_the_agenda_shows_the_next_accounts_cached_calendar_and_syncs_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut mailboxes = Vec::new();
    let mut commands = Vec::new();
    let mut calendars = Vec::new();
    for account in 0..2 {
        let path = dir.path().join(format!("{account}.db"));
        let own = appointment(
            &format!("a{account}"),
            jiff::Timestamp::now().round(jiff::Unit::Second).unwrap(),
        );
        cache_calendar(&path, std::slice::from_ref(&own));
        calendars.push(own);
        let (tx, rx) = mpsc::unbounded_channel();
        mailboxes.push(Mailbox {
            store: Store::open(&path).unwrap(),
            commands: tx,
        });
        commands.push(rx);
    }
    let mut mailboxes = Mailboxes::new(mailboxes, None, Box::new(io::sink()));
    let mut app = AppState {
        accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
        ..Default::default()
    };
    let effect = app.on_key(shift_c(), &mut Keymap::default());
    mailboxes.apply(&mut app, effect).unwrap();
    sent(&mut commands[0]);

    let effect = app.on_key(KeyEvent::from(KeyCode::Tab), &mut Keymap::default());
    mailboxes.apply(&mut app, effect).unwrap();

    let agenda = app.screen.agenda().unwrap();
    assert_eq!(agenda.address, "me@work.example");
    assert_eq!(agenda.appointments(), std::slice::from_ref(&calendars[1]));
    assert!(sent(&mut commands[0]).is_empty());
    assert_eq!(sent(&mut commands[1]), [Command::SyncCalendar]);
}

/// One empty mailbox, kept alive by the `TempDir`.
fn one_mailbox() -> (tempfile::TempDir, Mailboxes) {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::unbounded_channel();
    let mailbox = Mailbox {
        store: Store::open(&dir.path().join("cache.db")).unwrap(),
        commands: tx,
    };
    let mailboxes = Mailboxes::new(vec![mailbox], None, Box::new(io::sink()));
    (dir, mailboxes)
}

#[test]
fn installing_an_update_asks_the_installer() {
    let (_dir, mailboxes) = one_mailbox();
    let (installer, mut asked) = mpsc::unbounded_channel();
    let mut mailboxes = mailboxes.with_installer(installer);
    let mut app = AppState::default();

    mailboxes
        .apply(&mut app, Some(Effect::InstallUpdate))
        .unwrap();

    assert_eq!(asked.try_recv(), Ok(()));
}

#[test]
fn without_an_installer_nothing_is_installed() {
    let (_dir, mut mailboxes) = one_mailbox();
    let mut app = AppState::default();

    mailboxes
        .apply(&mut app, Some(Effect::InstallUpdate))
        .unwrap();

    assert_eq!(
        app.status,
        Some(Status::Info("tuilook is not updated from here".into()))
    );
}
