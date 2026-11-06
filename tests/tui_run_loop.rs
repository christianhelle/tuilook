use ratatui::{
    crossterm::event::{
        Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    layout::Rect,
};
use tokio::sync::mpsc;
use tuilook::core::fake;
use tuilook::model::Event;
use tuilook::store::Store;
use tuilook::tui::{
    app::{AppState, Pane, Status},
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
    run_loop::{
        AccountNews, UpdateNews, on_terminal_event, take_account_news, take_core_events,
        take_update,
    },
};

const AREA: Rect = Rect::new(0, 0, 100, 30);

struct Setup {
    app: AppState,
    mailboxes: Mailboxes,
    keymap: Keymap,
    _commands: mpsc::UnboundedReceiver<tuilook::core::Command>,
}

fn setup() -> Setup {
    let mut store = Store::open_in_memory().unwrap();
    store.upsert_folders(&[fake::folder("inbox")]).unwrap();
    let (commands, rx) = mpsc::unbounded_channel();
    let mailboxes = Mailboxes::new(
        vec![Mailbox { store, commands }],
        None,
        Box::new(std::io::sink()),
    );
    let mut app = AppState::default();
    app.accounts.push("me@outlook.com".to_owned());
    Setup {
        app,
        mailboxes,
        keymap: Keymap::default(),
        _commands: rx,
    }
}

fn key(code: KeyCode, kind: KeyEventKind) -> TermEvent {
    TermEvent::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> TermEvent {
    TermEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn deliver(s: &mut Setup, event: TermEvent) {
    on_terminal_event(&mut s.app, &mut s.mailboxes, &mut s.keymap, AREA, event).unwrap();
}

#[test]
fn a_newer_release_reaches_the_app() {
    let mut app = AppState::default();
    let (updates, mut received) = mpsc::channel(1);
    let version = semver::Version::new(0, 2, 0);

    updates
        .try_send(UpdateNews::Available(version.clone()))
        .unwrap();
    take_update(&mut app, &mut received);

    assert_eq!(app.update, Some(version));
}

#[test]
fn an_installed_release_asks_for_a_restart() {
    let mut app = AppState::default();
    let version = semver::Version::new(0, 2, 0);
    app.update = Some(version.clone());
    let (updates, mut received) = mpsc::channel(1);

    updates
        .try_send(UpdateNews::Installed(version.clone()))
        .unwrap();
    take_update(&mut app, &mut received);

    assert_eq!(app.update, None);
    assert_eq!(app.installed, Some(version));
    assert_eq!(
        app.status,
        Some(Status::Info(
            "Installed tuilook 0.2.0; restart otlk to use it".into()
        ))
    );
}

#[test]
fn updating_the_latest_release_says_so() {
    let mut app = AppState::default();
    let (updates, mut received) = mpsc::channel(1);

    updates.try_send(UpdateNews::UpToDate).unwrap();
    take_update(&mut app, &mut received);

    assert_eq!(
        app.status,
        Some(Status::Info("tuilook is up to date".into()))
    );
}

#[test]
fn a_failed_update_says_why() {
    let mut app = AppState::default();
    let (updates, mut received) = mpsc::channel(1);

    updates
        .try_send(UpdateNews::Failed("offline".into()))
        .unwrap();
    take_update(&mut app, &mut received);

    assert_eq!(
        app.status,
        Some(Status::Error("Could not update: offline".into()))
    );
}

#[test]
fn a_check_still_running_changes_nothing() {
    let mut app = AppState::default();
    let (_updates, mut received) = mpsc::channel(1);

    take_update(&mut app, &mut received);

    assert_eq!(app.update, None);
}

#[test]
fn a_check_that_found_nothing_changes_nothing() {
    let mut app = AppState::default();
    let (updates, mut received) = mpsc::channel(1);
    drop(updates);

    take_update(&mut app, &mut received);

    assert_eq!(app.update, None);
}

#[test]
fn a_key_press_reaches_the_app() {
    let mut s = setup();

    deliver(&mut s, key(KeyCode::Char('q'), KeyEventKind::Press));

    assert!(s.app.should_quit);
}

#[test]
fn a_key_release_does_nothing_as_windows_reports_those_too() {
    let mut s = setup();

    deliver(&mut s, key(KeyCode::Char('q'), KeyEventKind::Release));

    assert!(!s.app.should_quit);
}

#[test]
fn the_panes_are_measured_before_a_key_acts_on_them() {
    let mut s = setup();
    assert_eq!(s.app.viewport.reader, 0);

    deliver(&mut s, key(KeyCode::Tab, KeyEventKind::Press));

    assert_eq!(s.app.viewport.reader, 27);
}

#[test]
fn a_left_click_focuses_the_pane_under_it() {
    let mut s = setup();

    deliver(
        &mut s,
        mouse(MouseEventKind::Down(MouseButton::Left), 90, 10),
    );

    assert_eq!(s.app.focus, Pane::Reader);
}

#[test]
fn other_mouse_events_do_nothing() {
    let mut s = setup();

    deliver(&mut s, mouse(MouseEventKind::Moved, 90, 10));
    deliver(
        &mut s,
        mouse(MouseEventKind::Down(MouseButton::Right), 90, 10),
    );

    assert_eq!(s.app.focus, Pane::Folders);
}

#[test]
fn what_each_core_reported_is_shown() {
    let mut s = setup();
    let (tx, rx) = mpsc::channel(4);
    tx.try_send(Event::Error("offline".to_owned())).unwrap();
    let mut events = vec![rx];

    take_core_events(&mut s.app, &mut s.mailboxes, &mut events).unwrap();

    assert_eq!(s.app.status, Some(Status::Error("offline".to_owned())));
}

#[test]
fn signing_in_shows_the_code_then_the_account() {
    use tuilook::model::Action;
    use tuilook::tui::{account_manager::Login, dialog::Dialog};
    let mut app = AppState {
        accounts: vec!["me@outlook.com".to_owned()],
        ..AppState::default()
    };
    app.update(Action::ManageAccounts);
    app.on_key(KeyEvent::from(KeyCode::Char('a')), &mut Keymap::default());
    app.on_key(KeyEvent::from(KeyCode::Enter), &mut Keymap::default());
    let (news, mut received) = mpsc::unbounded_channel();

    news.send(AccountNews::Code("Enter ABC".to_owned()))
        .unwrap();
    news.send(AccountNews::SignedIn {
        address: "new@outlook.com".to_owned(),
        login: Login::default(),
    })
    .unwrap();
    take_account_news(&mut app, &mut received);

    let Some(Dialog::Accounts(manager)) = &app.dialog else {
        panic!("no accounts dialog");
    };
    assert_eq!(manager.accounts.len(), 2);
    assert_eq!(
        app.status,
        Some(Status::Info("Signed in as new@outlook.com".to_owned()))
    );
}
