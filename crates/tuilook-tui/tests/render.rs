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

#[test]
fn reading_pane_shows_the_loaded_body() {
    let mut app = sample_app();
    app.body_loaded(
        MessageId("m1".to_owned()),
        "Dear Charles,\n\nThe engine weaves algebraic patterns.\n\n[1]: https://example.com"
            .to_owned(),
    );

    insta::assert_snapshot!(render(&app));
}

#[test]
fn status_bar_shows_the_last_error() {
    let mut app = sample_app();
    app.show_error("Graph returned 503 Service Unavailable".to_owned());

    insta::assert_snapshot!(render(&app));
}

#[test]
fn subfolders_are_indented_under_their_parent() {
    let mut receipts = folder("receipts", "Receipts", None, 1);
    receipts.parent_id = Some(FolderId("inbox".to_owned()));
    let mut year = folder("2026", "2026", None, 0);
    year.parent_id = Some(FolderId("receipts".to_owned()));
    let mut app = sample_app();
    app.folders = vec![
        folder("inbox", "Inbox", Some(WellKnown::Inbox), 2),
        receipts,
        year,
        folder("proj", "Projects", None, 0),
    ];

    insta::assert_snapshot!(render(&app));
}

#[test]
fn a_scrolled_reading_pane_starts_further_down() {
    let mut app = sample_app();
    let body: Vec<String> = (1..=20).map(|i| format!("Line {i}")).collect();
    app.body_loaded(MessageId("m1".to_owned()), body.join("\n"));
    app.reader_scroll = 5;

    insta::assert_snapshot!(render(&app));
}

#[test]
fn viewport_measures_what_each_pane_can_show() {
    let mut app = sample_app();
    let long_line = "word ".repeat(20); // 100 columns: three rows at 34 wide
    app.body_loaded(MessageId("m1".to_owned()), format!("Hi\n{long_line}"));

    let viewport = ui::viewport(ratatui::layout::Rect::new(0, 0, 90, 12), &app);

    assert_eq!(
        viewport,
        tuilook_tui::app::Viewport {
            folders: 9,
            messages: 4,
            reader: 9,
            // From, Subject, blank, "Hi", then the long line wrapped in three
            reader_lines: 7,
        }
    );
}

#[test]
fn a_zoomed_pane_fills_the_screen() {
    let mut app = sample_app();
    app.focus = Pane::Reader;
    app.zoomed = true;

    insta::assert_snapshot!(render(&app));
}

#[test]
fn hidden_folders_leave_the_messages_and_reading_pane() {
    let mut app = sample_app();
    app.folders_hidden = true;
    app.focus = Pane::Messages;

    insta::assert_snapshot!(render(&app));
}

#[test]
fn paging_a_zoomed_pane_moves_by_the_whole_screen() {
    let mut app = sample_app();
    app.focus = Pane::Messages;
    app.zoomed = true;

    let viewport = ui::viewport(ratatui::layout::Rect::new(0, 0, 90, 12), &app);

    assert_eq!(viewport.messages, 4);
    assert_eq!(viewport.folders, 0);
    assert_eq!(viewport.reader, 0);
}

#[test]
fn marked_messages_show_a_dot_and_a_count() {
    let mut app = sample_app();
    app.focus = Pane::Messages;
    app.update(tuilook_model::Action::ToggleMark);

    insta::assert_snapshot!(render(&app));
}
