use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent},
};
use tuilook::model::{Folder, FolderId, Importance, Message, MessageId, WellKnown, sort_folders};
use tuilook::tui::{
    app::{AppState, Pane},
    demo,
    groups::MessageRow,
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
    app.update(tuilook::model::Action::SelectNext);

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

/// Inbox > Receipts > 2026, then Projects.
fn with_subfolders() -> AppState {
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
    app
}

#[test]
fn subfolders_are_indented_under_their_parent() {
    let mut app = with_subfolders();
    app.update(tuilook::model::Action::Expand);
    app.update(tuilook::model::Action::Expand);
    app.update(tuilook::model::Action::Expand);

    insta::assert_snapshot!(render(&app));
}

#[test]
fn a_collapsed_folder_counts_the_unread_mail_inside_it() {
    insta::assert_snapshot!(render(&with_subfolders()));
}

#[test]
fn a_click_finds_the_folder_drawn_under_it_past_collapsed_ones() {
    let mut app = with_subfolders();
    let area = ratatui::layout::Rect::new(0, 0, 90, 12);

    // The empty favorites and unread section take the first six rows.
    let hit = ui::hit(area, &app, 4, 8).unwrap();
    assert_eq!(hit.item, Some(3), "Projects, below the collapsed Inbox");

    app.update(tuilook::model::Action::Expand);
    assert_eq!(ui::hit(area, &app, 4, 8).unwrap().item, Some(1), "Receipts");
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
        tuilook::tui::app::Viewport {
            folders: 3,
            messages: 4,
            reader: 9,
            // From, Subject, blank, "Hi", then the long line wrapped in three
            reader_lines: 7,
            reader_width: 34,
            compose: 0,
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
    app.update(tuilook::model::Action::ToggleMark);

    insta::assert_snapshot!(render(&app));
}

#[test]
fn deleting_asks_first() {
    let mut app = sample_app();
    app.focus = Pane::Messages;
    app.update(tuilook::model::Action::Delete);

    insta::assert_snapshot!(render(&app));
}

#[test]
fn deleting_for_good_warns_it_cannot_be_undone() {
    let mut app = sample_app();
    app.focus = Pane::Messages;
    app.update(tuilook::model::Action::MarkAll);
    app.update(tuilook::model::Action::DeletePermanently);

    insta::assert_snapshot!(render(&app));
}

#[test]
fn question_mark_shows_the_keys() {
    let mut app = sample_app();
    app.update(tuilook::model::Action::Help);
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    insta::assert_snapshot!(terminal.backend().to_string());
}

#[test]
fn the_help_is_wide_enough_for_its_longest_line() {
    let inside = usize::from(ui::HELP_WIDTH) - 2;

    assert!(ui::help_line_width(&ui::HELP) <= inside);
}

fn press(app: &mut AppState, keys: &str) {
    let mut keymap = tuilook::tui::keymap::Keymap::default();
    for c in keys.chars() {
        let code = match c {
            '⎋' => KeyCode::Esc,
            '⇥' => KeyCode::Tab,
            '⏎' => KeyCode::Enter,
            c => KeyCode::Char(c),
        };
        // As the event loop does, measure the screen before each key.
        app.viewport = ui::viewport(ratatui::layout::Rect::new(0, 0, 90, 16), app);
        app.on_key(KeyEvent::from(code), &mut keymap);
    }
}

fn render_tall(app: &AppState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(90, 16)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn a_new_message_takes_over_the_screen() {
    let mut app = sample_app();
    press(&mut app, "cada@example.com⇥⇥Lunch⇥iShall we?⏎Friday works");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn a_reply_notes_the_original_is_quoted_and_shows_the_vim_mode() {
    let mut app = sample_app();
    press(&mut app, "rAgreed⎋");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn closing_a_written_message_asks_first() {
    let mut app = sample_app();
    press(&mut app, "rAgreed⎋⎋");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn question_mark_while_writing_shows_the_writing_keys() {
    let mut app = sample_app();
    press(&mut app, "r⎋?");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn the_writing_help_fits_too() {
    let inside = usize::from(ui::HELP_WIDTH) - 2;

    assert!(ui::help_line_width(&ui::COMPOSE_HELP) <= inside);
    assert!(ui::help_line_width(&ui::SIGNATURE_HELP) <= inside);
}

#[test]
fn a_click_finds_the_message_drawn_under_it_in_a_scrolled_list() {
    let mut app = sample_app();
    let names: Vec<String> = (0..10).map(|i| format!("Sender{i}")).collect();
    app.set_messages(
        names
            .iter()
            .enumerate()
            .map(|(i, name)| message(&format!("m{i}"), name, "Subject", ""))
            .collect(),
    );
    app.focus = Pane::Messages;
    for _ in 0..7 {
        app.update(tuilook::model::Action::SelectNext);
    }
    let area = ratatui::layout::Rect::new(0, 0, 90, 12);
    let screen = render(&app);
    // The message pane starts at column 17; its first message row is screen row 1.
    let first_row = screen.lines().nth(1).unwrap();

    let hit = ui::hit(area, &app, 30, 1).unwrap();

    assert_eq!(hit.pane, Pane::Messages);
    // The hit is a row of the list, which starts with the date header.
    let item = hit.item.unwrap();
    let MessageRow::Message(index) = app.message_rows()[item] else {
        panic!("row {item} is a message");
    };
    assert!(
        first_row.contains(&names[index]),
        "{first_row} shows Sender{index}"
    );
    assert_eq!(
        ui::hit(area, &app, 30, 2).unwrap().item,
        Some(item),
        "its subject line"
    );
    assert_eq!(ui::hit(area, &app, 30, 3).unwrap().item, Some(item + 1));
}

#[test]
fn a_click_on_a_border_or_past_the_last_row_hits_no_row() {
    let app = sample_app();
    let area = ratatui::layout::Rect::new(0, 0, 90, 12);

    assert_eq!(ui::hit(area, &app, 30, 0).unwrap().item, None, "the border");
    assert_eq!(
        ui::hit(area, &app, 30, 8).unwrap().item,
        None,
        "no message there"
    );
    assert_eq!(
        ui::hit(area, &app, 2, 8).unwrap().item,
        Some(1),
        "Sent Items"
    );
    assert_eq!(
        ui::hit(area, &app, 2, 1).unwrap().item,
        None,
        "the hint in the empty favorites"
    );
    assert_eq!(ui::hit(area, &app, 60, 3).unwrap().pane, Pane::Reader);
    assert_eq!(ui::hit(area, &app, 30, 11), None, "the status bar");
}

#[test]
fn a_zoomed_pane_takes_every_click() {
    let mut app = sample_app();
    app.focus = Pane::Reader;
    app.zoomed = true;
    let area = ratatui::layout::Rect::new(0, 0, 90, 12);

    assert_eq!(ui::hit(area, &app, 2, 2).unwrap().pane, Pane::Reader);
}

#[test]
fn a_blocked_send_says_what_to_do() {
    let mut app = sample_app();
    press(&mut app, "rThanks");
    app.send_blocked("ErrorMessageSubmissionBlocked: Message submission blocked".to_owned());

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn a_burst_of_sends_asks_before_the_next() {
    let mut app = sample_app();
    for _ in 0..tuilook::tui::app::PACE_LIMIT {
        app.mail_sent(std::time::Instant::now());
    }
    press(&mut app, "rThanks");
    let ctrl_s = KeyEvent::new(
        KeyCode::Char('s'),
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    );
    app.on_key(ctrl_s, &mut tuilook::tui::keymap::Keymap::default());

    insta::assert_snapshot!(render_tall(&app));
}

fn two_accounts() -> AppState {
    let mut app = AppState {
        accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
        ..Default::default()
    };
    app.set_account_folders(
        0,
        vec![
            folder("inbox", "Inbox", Some(WellKnown::Inbox), 2),
            folder("sent", "Sent Items", Some(WellKnown::SentItems), 0),
        ],
    );
    app.set_account_folders(1, vec![folder("work", "Inbox", Some(WellKnown::Inbox), 5)]);
    app.set_messages(vec![message("m1", "Ada", "Analytical engine", "Notes")]);
    app
}

#[test]
fn each_account_heads_its_own_folders() {
    insta::assert_snapshot!(render(&two_accounts()));
}

#[test]
fn a_message_says_which_account_it_is_sent_from() {
    let mut app = two_accounts();
    app.selected_folder = app.folders.iter().position(|f| f.id.0 == "work").unwrap();
    press(&mut app, "c");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn shift_s_writes_the_accounts_signature() {
    let mut app = two_accounts();
    app.selected_folder = app.folders.iter().position(|f| f.id.0 == "work").unwrap();
    press(&mut app, "SAda Lovelace⏎Analytical Engines");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn a_new_message_starts_with_the_signature() {
    let mut app = two_accounts();
    app.signatures = vec![
        String::new(),
        "Ada Lovelace
Analytical Engines"
            .to_owned(),
    ];
    app.selected_folder = app.folders.iter().position(|f| f.id.0 == "work").unwrap();
    press(&mut app, "c");

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn the_demo_has_several_accounts_with_nested_folders() {
    let mut app = AppState {
        accounts: demo::accounts().into_iter().map(|(a, _)| a).collect(),
        ..Default::default()
    };
    for (account, (_, backend)) in demo::accounts().into_iter().enumerate() {
        let mut folders = backend.folders;
        sort_folders(&mut folders);
        app.set_account_folders(account, folders);
    }
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    insta::assert_snapshot!(terminal.backend().to_string());
}

/// Both inboxes and Sent Items in the favorites, with the work inbox selected.
fn with_favorites() -> AppState {
    let mut app = two_accounts();
    app.focus = Pane::Folders;
    for id in ["inbox", "work", "sent"] {
        app.selected_folder = app.folders.iter().position(|f| f.id.0 == id).unwrap();
        app.update(tuilook::model::Action::ToggleFavorite);
    }
    app.selected_folder = 1;
    app.status = None;
    app
}

#[test]
fn favorites_name_the_account_when_names_clash() {
    let mut terminal = Terminal::new(TestBackend::new(90, 16)).unwrap();
    terminal
        .draw(|frame| ui::draw(frame, &with_favorites()))
        .unwrap();

    insta::assert_snapshot!(terminal.backend().to_string());
}

#[test]
fn a_click_in_the_favorites_selects_the_favorite() {
    let mut app = with_favorites();
    let area = ratatui::layout::Rect::new(0, 0, 90, 16);

    let hit = ui::hit(area, &app, 4, 1).unwrap();
    assert_eq!(hit.item, Some(0));
    app.on_mouse(tuilook::tui::app::Mouse::Click, Some(hit));

    assert_eq!(app.shown_folder(), Some(FolderId("inbox".to_owned())));
    let first_account = app
        .folders
        .iter()
        .position(|f| f.id.0 == "account:me@outlook.com");
    assert_eq!(
        ui::hit(area, &app, 4, 11).unwrap().item,
        first_account,
        "the first account, below the favorites and the unread section"
    );
}

#[test]
fn mail_text_draws_nothing_a_terminal_could_count_differently() {
    let mut app = sample_app();
    let padding = "\u{034F}\u{00AD}\u{200C}\u{FEFF}".repeat(50);
    app.set_messages(vec![message(
        "m1",
        "Ada\u{200B}",
        "⚠\u{FE0F} Engine\r\nnotes",
        &format!("Notes{padding}\tend"),
    )]);
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let cells = terminal.backend().buffer().content();

    assert!(
        cells
            .iter()
            .all(|cell| cell.symbol() == ui::shown(cell.symbol()))
    );
    assert!(terminal.backend().to_string().contains("Notes    end"));
}

#[test]
fn typing_a_search_shows_the_query() {
    let mut app = sample_app();

    press(&mut app, "/eng");

    insta::assert_snapshot!(render(&app));
}

#[test]
fn search_results_say_where_each_message_is() {
    let mut app = two_accounts();
    press(&mut app, "/ada⏎");
    let mut sent = message("m2", "Ada", "Re: Analytical engine", "Thanks");
    sent.folder_id = FolderId("sent".to_owned());
    let mut work = message("m3", "Ada", "Engine budget", "Numbers");
    work.folder_id = FolderId("work".to_owned());
    app.search_results(vec![
        (0, message("m1", "Ada", "Analytical engine", "Notes")),
        (0, sent),
        (1, work),
    ]);
    app.focus = Pane::Messages;

    insta::assert_snapshot!(render_tall(&app));
}

#[test]
fn a_search_that_finds_nothing_says_so() {
    let mut app = sample_app();
    press(&mut app, "/zzz");
    app.search_results(vec![]);

    insta::assert_snapshot!(render(&app));
}

/// The leftmost place `text` is drawn, which is in the message list when the
/// reading pane shows it too.
fn leftmost(terminal: &Terminal<TestBackend>, text: &str) -> ratatui::style::Style {
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width as usize;
    let rows: Vec<String> = buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect();
    let (x, y) = rows
        .iter()
        .enumerate()
        .filter_map(|(y, row)| row.find(text).map(|x| (row[..x].chars().count(), y)))
        .min()
        .unwrap_or_else(|| panic!("{text} is not drawn"));
    buffer.content()[y * width + x].style()
}

#[test]
fn read_mail_is_dimmed_and_unread_mail_is_bold() {
    use ratatui::style::{Color, Modifier};
    let mut app = sample_app();
    let mut read = message("m2", "Grace", "Compiler draft", "First pass attached");
    read.is_read = true;
    app.set_messages(vec![
        message("m1", "Ada", "Analytical engine", "Notes on the engine"),
        read,
    ]);
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    for unread in ["Ada", "Analytical engine"] {
        let style = leftmost(&terminal, unread);
        assert!(style.add_modifier.contains(Modifier::BOLD), "{unread}");
    }
    for read in ["Grace", "Compiler draft"] {
        let style = leftmost(&terminal, read);
        assert_eq!(style.fg, Some(Color::DarkGray), "{read}");
        assert!(!style.add_modifier.contains(Modifier::BOLD), "{read}");
    }
}

#[test]
fn the_unread_section_lists_folders_with_unread_mail() {
    let mut app = two_accounts();
    app.focus = Pane::Folders;
    app.selected_folder = app
        .folders
        .iter()
        .position(|f| f.id.0 == "unread:work")
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(90, 16)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    insta::assert_snapshot!(terminal.backend().to_string());
}

#[test]
fn picked_text_in_the_reader_is_highlighted() {
    use ratatui::style::Modifier;
    let mut app = sample_app();
    app.body_loaded(MessageId("m1".to_owned()), "Dear Charles,".to_owned());
    app.focus = Pane::Reader;
    press(&mut app, "vj");
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    let reversed = |text| {
        leftmost(&terminal, text)
            .add_modifier
            .contains(Modifier::REVERSED)
    };
    assert!(reversed("From: Ada"), "the first line is picked");
    assert!(reversed("Subject:"), "up to the cursor");
    assert!(!reversed("ubject:"), "but not past it");
    assert!(!reversed("Dear"));
}

#[test]
fn picking_in_the_reader_shows_the_mode_and_how_to_copy() {
    let mut app = sample_app();
    app.focus = Pane::Reader;
    press(&mut app, "V");

    insta::assert_snapshot!(render(&app));
}

#[test]
fn the_focused_reader_shows_its_cursor() {
    use ratatui::style::Modifier;
    let mut app = sample_app();
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    let reversed = |terminal: &Terminal<TestBackend>, text| {
        leftmost(terminal, text)
            .add_modifier
            .contains(Modifier::REVERSED)
    };

    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    assert!(!reversed(&terminal, "From: Ada"), "only while focused");

    app.focus = Pane::Reader;
    press(&mut app, "jj");
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let cursor = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|c| c.style().add_modifier.contains(Modifier::REVERSED));
    assert_eq!(
        cursor.count(),
        1,
        "the cursor is one cell, on the empty line"
    );
    assert!(!reversed(&terminal, "From: Ada"));

    press(&mut app, "k");
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    assert!(reversed(&terminal, "Subject:"));
    assert!(!reversed(&terminal, "ubject:"));
}

#[test]
fn the_focused_reader_shows_normal_mode() {
    let mut app = sample_app();
    app.focus = Pane::Reader;

    insta::assert_snapshot!(render(&app));
}
