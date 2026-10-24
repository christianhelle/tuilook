use ratatui::{Terminal, backend::TestBackend};
use tuilook_model::{Folder, FolderId, Importance, Message, MessageId, WellKnown};
use tuilook_tui::{
    app::{AppState, Pane},
    ui,
};

fn folder(id: &str, name: &str, well_known: Option<WellKnown>, unread: u32) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: None,
        name: name.to_owned(),
        well_known,
        unread,
        total: 10,
    }
}

fn message(id: &str, from: &str, subject: &str, preview: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("inbox".to_owned()),
        conversation_id: String::new(),
        subject: subject.to_owned(),
        from_name: from.to_owned(),
        from_addr: format!("{}@example.com", from.to_lowercase()),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        is_read: false,
        flagged: false,
        importance: Importance::Normal,
        has_attachments: false,
        preview: preview.to_owned(),
    }
}

fn sample_app() -> AppState {
    let mut app = AppState {
        folders: vec![
            folder("inbox", "Inbox", Some(WellKnown::Inbox), 2),
            folder("sent", "Sent Items", Some(WellKnown::SentItems), 0),
            folder("proj", "Projects", None, 0),
        ],
        ..Default::default()
    };
    app.set_messages(vec![
        message("m1", "Ada", "Analytical engine", "Notes on the engine"),
        message("m2", "Grace", "Compiler draft", "First pass attached"),
    ]);
    app
}

fn render(app: &AppState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn shows_folders_messages_and_reading_pane() {
    insta::assert_snapshot!(render(&sample_app()));
}

#[test]
fn focused_pane_has_a_thick_border() {
    let mut app = sample_app();
    app.focus = Pane::Messages;
    app.update(tuilook_model::Action::SelectNext);

    insta::assert_snapshot!(render(&app));
}

#[test]
fn empty_folder_says_so() {
    let mut app = sample_app();
    app.set_messages(vec![]);

    insta::assert_snapshot!(render(&app));
}
