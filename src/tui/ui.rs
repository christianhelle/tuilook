//! Renders [`AppState`] into a frame. Pure: no IO, no state changes.

use crate::model::{ComposeKind, Message, folder_depths};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::tui::{
    app::{
        AppState, Confirm, Hit, PACE_WINDOW, Pane, Status, Viewport, is_account_row,
        is_favorite_row,
    },
    compose::{Compose, Field},
    vim::VimMode,
};

/// Mail text made safe to draw. Mail is full of characters that take no room
/// (zero-width joiners, soft hyphens, variation selectors), often hundreds
/// of them padding a newsletter's preview. Ratatui counts them as no width,
/// but some terminals (WezTerm, for one) give them a cell, so every row they
/// are on pushes past its pane. Ratatui drops tabs, so they become spaces.
pub fn shown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\n' => out.push(c),
            c if c.is_control() || takes_no_room(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// Unicode's default-ignorable characters: drawn as nothing at all.
fn takes_no_room(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// Each message takes this many rows in the message list.
const MESSAGE_ROWS: usize = 2;

/// Where each pane goes. A pane that is not shown gets an empty area.
#[derive(Debug, Default)]
struct Panes {
    /// The favorites, above the accounts' folders.
    favorites: Rect,
    folders: Rect,
    messages: Rect,
    reader: Rect,
    /// The message being written, which takes the place of the panes.
    compose: Rect,
    status: Rect,
}

/// Lays out the screen. Drawing, paging and mouse clicks all use this, so
/// they agree on where everything is.
fn panes(area: Rect, app: &AppState) -> Panes {
    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let mut panes = Panes {
        status,
        ..Default::default()
    };
    if app.compose.is_some() {
        panes.compose = main;
        return panes;
    }
    match app.zoomed_pane() {
        Some(Pane::Folders) => panes.folders = main,
        Some(Pane::Messages) => panes.messages = main,
        Some(Pane::Reader) => panes.reader = main,
        None if app.folders_hidden => {
            [panes.messages, panes.reader] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(main);
        }
        None => {
            [panes.folders, panes.messages, panes.reader] = Layout::horizontal([
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ])
            .areas(main);
        }
    }
    if !panes.folders.is_empty() {
        [panes.favorites, panes.folders] = split_favorites(panes.folders, app);
    }
    panes
}

/// Splits the folder pane into the favorites, as tall as they need up to
/// half the pane, and the accounts' folders below them.
fn split_favorites(area: Rect, app: &AppState) -> [Rect; 2] {
    // An empty favorites box still has a row, for the hint.
    let rows = u16::try_from(app.favorite_count().max(1)).unwrap_or(u16::MAX);
    let height = rows.saturating_add(2).min(area.height / 2);
    Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area)
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let Panes {
        favorites,
        folders,
        messages,
        reader,
        compose,
        status,
    } = panes(frame.area(), app);

    if !folders.is_empty() {
        draw_folders(frame, app, favorites, folders);
    }
    if !messages.is_empty() {
        draw_messages(frame, app, messages);
    }
    if !reader.is_empty() {
        draw_reader(frame, app, reader);
    }
    if let Some(message) = &app.compose {
        draw_compose(frame, message, compose);
    }
    frame.render_widget(status_line(app), status);
    if let Some(search) = app.search.as_ref().filter(|s| s.editing) {
        let typed = u16::try_from(Line::raw(search.query.as_str()).width()).unwrap_or(u16::MAX);
        let x = status.x.saturating_add(2).saturating_add(typed);
        frame.set_cursor_position((x.min(status.right().saturating_sub(1)), status.y));
    }

    if app.show_help {
        let help: &[(&str, &str)] = match app.compose {
            Some(_) => &COMPOSE_HELP,
            None => &HELP,
        };
        draw_help(frame, help);
    }
    match &app.confirm {
        Some(Confirm::Delete { ids, permanent }) => {
            draw_confirm_delete(frame, ids.len(), *permanent);
        }
        Some(Confirm::Discard) => draw_confirm_discard(frame),
        Some(Confirm::SendAnyway { sent_lately, .. }) => draw_confirm_burst(frame, *sent_lately),
        None => {}
    }
    if let Some(said) = &app.blocked {
        draw_blocked(frame, said);
    }
}

/// A dialog asking whether to send one more message in a burst of them.
fn draw_confirm_burst(frame: &mut Frame, sent: usize) {
    let lines = vec![
        Line::raw(format!(
            "You have sent {sent} messages in the last {} minutes.",
            PACE_WINDOW.as_secs() / 60
        )),
        Line::raw("Outlook.com may block accounts that send in bursts."),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y / Enter", Style::new().fg(Color::Yellow)),
            Span::raw(" send anyway   "),
            Span::styled("Esc", Style::new().fg(Color::Cyan)),
            Span::raw(" or any other key wait"),
        ]),
    ];
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(60), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title("Send another?")
                .border_style(Style::new().fg(Color::Yellow)),
        ),
        area,
    );
}

/// A dialog saying Outlook.com refuses to send for now, and what to do.
fn draw_blocked(frame: &mut Frame, said: &str) {
    let lines = vec![
        Line::raw("Outlook.com has blocked sending from this account,"),
        Line::raw("usually because recent mail looked like spam to it."),
        Line::raw("Nothing was sent; your message is still open."),
        Line::raw(""),
        Line::from(vec![
            Span::raw("Unblock it at "),
            Span::styled("https://account.live.com", Style::new().fg(Color::Cyan)),
            Span::raw(","),
        ]),
        Line::raw("then wait a little and press Ctrl-S to try again."),
        Line::raw(""),
        Line::styled(
            format!("Outlook said: {said}"),
            Style::new().fg(Color::DarkGray),
        ),
        Line::raw(""),
        Line::styled("Press any key to close", Style::new().fg(Color::DarkGray)),
    ];
    let width = frame.area().width.saturating_sub(4).min(64);
    let inner = usize::from(width.saturating_sub(2)).max(1);
    // Rows the lines take wrapped, with room for words moved to the next row.
    let rows: usize = lines
        .iter()
        .map(|line| line.width().div_ceil(inner).max(1) + usize::from(line.width() > inner))
        .sum();
    let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title("Sending blocked")
                .border_style(Style::new().fg(Color::Red)),
        ),
        area,
    );
}

fn status_line(app: &AppState) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(compose) = &app.compose
        && compose.field == Field::Body
    {
        let mode = compose.body.vim.mode;
        let color = match mode {
            VimMode::Normal => Color::Blue,
            VimMode::Insert => Color::Green,
            VimMode::Visual | VimMode::VisualLine => Color::Magenta,
        };
        let badge = Style::new().fg(Color::Black).bg(color);
        spans.push(Span::styled(format!(" {} ", mode.label()), badge));
    }
    if let Some(search) = app.search.as_ref().filter(|s| s.editing) {
        spans.push(Span::styled(" /", Style::new().fg(Color::Cyan)));
        spans.push(Span::raw(search.query.clone()));
        return Line::from(spans);
    }
    spans.push(match &app.status {
        Some(Status::Error(error)) => {
            Span::styled(format!(" {error}"), Style::new().fg(Color::Red))
        }
        Some(Status::Info(info) | Status::Busy(info)) => Span::raw(format!(" {info}")),
        None if app.compose.is_some() => {
            Span::raw(" Ctrl-S send · Tab next field · Esc close · ? help")
        }
        None => Span::raw(
            " ? help · Tab switch pane · Enter open · c write · r reply · d delete · q quit",
        ),
    });
    Line::from(spans)
}

/// Where each part of the message being written goes.
struct ComposeAreas {
    /// The account it is sent from, when there are several.
    from: Rect,
    to: Rect,
    cc: Rect,
    subject: Rect,
    rule: Rect,
    /// Says the original is quoted below, for a reply or forward.
    note: Rect,
    body: Rect,
}

fn compose_areas(area: Rect, compose: &Compose) -> ComposeAreas {
    let inner = Block::bordered().inner(area);
    let note = u16::from(compose.quotes_original());
    let from = u16::from(compose.from.is_some());
    let [from, to, cc, subject, rule, note, body] = Layout::vertical([
        Constraint::Length(from),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(note),
        Constraint::Fill(1),
    ])
    .areas(inner);
    ComposeAreas {
        from,
        to,
        cc,
        subject,
        rule,
        note,
        body,
    }
}

/// Width of the labels in front of the header fields.
const LABEL_WIDTH: u16 = 9;

fn draw_compose(frame: &mut Frame, compose: &Compose, area: Rect) {
    let title = match compose.kind {
        ComposeKind::New => "New message",
        ComposeKind::Reply(_) => "Reply",
        ComposeKind::ReplyAll(_) => "Reply all",
        ComposeKind::Forward(_) => "Forward",
    };
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Thick)
            .title(format!(" {title} ")),
        area,
    );
    let areas = compose_areas(area, compose);
    if let Some(from) = &compose.from {
        let label = format!("{:<width$}", "From:", width = usize::from(LABEL_WIDTH));
        let line = Line::from(vec![
            Span::styled(label, Style::new().fg(Color::DarkGray)),
            Span::raw(from.clone()),
        ]);
        frame.render_widget(line, areas.from);
    }
    let fields = [
        (Field::To, "To:", &compose.to, areas.to),
        (Field::Cc, "Cc:", &compose.cc, areas.cc),
        (Field::Subject, "Subject:", &compose.subject, areas.subject),
    ];
    for (field, label, input, row) in fields {
        let focused = compose.field == field;
        let label_style = if focused {
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        // Long text scrolls so the cursor stays in view.
        let width = usize::from(row.width.saturating_sub(LABEL_WIDTH)).max(1);
        let skip = if focused {
            input.cursor().saturating_sub(width - 1)
        } else {
            0
        };
        let text: String = input.text().chars().skip(skip).collect();
        let line = Line::from(vec![
            Span::styled(
                format!("{label:<width$}", width = usize::from(LABEL_WIDTH)),
                label_style,
            ),
            Span::raw(text),
        ]);
        frame.render_widget(line, row);
        if focused {
            let column = u16::try_from(input.cursor() - skip).unwrap_or(u16::MAX);
            frame.set_cursor_position((row.x + LABEL_WIDTH + column, row.y));
        }
    }
    let rule = "─".repeat(usize::from(areas.rule.width));
    frame.render_widget(
        Line::styled(rule, Style::new().fg(Color::DarkGray)),
        areas.rule,
    );
    if !areas.note.is_empty() {
        let note = Style::new()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC);
        frame.render_widget(
            Line::styled("The original message is quoted below what you write", note),
            areas.note,
        );
    }
    draw_body(frame, compose, areas.body);
}

/// The body being written, scrolled so the cursor is in view.
fn draw_body(frame: &mut Frame, compose: &Compose, area: Rect) {
    let editor = &compose.body;
    let height = usize::from(area.height);
    let width = usize::from(area.width).max(1);
    let (row, column) = editor.cursor();
    let top = editor.scroll(height);
    let left = column.saturating_sub(width - 1);
    let selection = editor.vim.selection((row, column));
    let picked = Style::new().add_modifier(Modifier::REVERSED);
    let lines: Vec<Line> = editor
        .lines()
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(index, line)| {
            let chars = line.chars().enumerate().skip(left);
            let spans = chars.map(|(at, c)| {
                let selected = selection.is_some_and(|(first, last)| {
                    (first.0, first.1) <= (index, at) && (index, at) <= (last.0, last.1)
                });
                if selected {
                    Span::styled(c.to_string(), picked)
                } else {
                    Span::raw(c.to_string())
                }
            });
            Line::from(spans.collect::<Vec<_>>())
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
    if compose.field == Field::Body {
        let x = u16::try_from(column - left).unwrap_or(u16::MAX);
        let y = u16::try_from(row - top).unwrap_or(u16::MAX);
        frame.set_cursor_position((area.x + x, area.y + y));
    }
}

/// A dialog asking whether to close the message being written.
fn draw_confirm_discard(frame: &mut Frame) {
    let lines = vec![
        Line::raw("Close the message and lose what you wrote?"),
        Line::raw(""),
        Line::from(vec![
            Span::styled("y / Enter", Style::new().fg(Color::Red)),
            Span::raw(" discard   "),
            Span::styled("Esc", Style::new().fg(Color::Cyan)),
            Span::raw(" or any other key keep writing"),
        ]),
    ];
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(60), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title("Discard message")
                .border_style(Style::new().fg(Color::Red)),
        ),
        area,
    );
}

/// What each key does while writing a message, shown with `?` in normal mode.
pub const COMPOSE_HELP: [(&str, &str); 11] = [
    ("Tab / Shift-Tab", "Next field / previous field"),
    ("Enter  ↑ ↓", "Move between To, Cc and Subject"),
    ("a, b; c", "Separate several addresses"),
    ("i a I A o O", "Type in the body, insert mode"),
    ("Esc", "Back to normal mode"),
    ("h j k l w b e 0 $ gg G", "Move in the body"),
    ("dd cw x p u Ctrl-R v V", "Edit vim style"),
    ("Ctrl-S", "Send"),
    (
        "Esc in normal mode",
        "Close, asking first if anything was written",
    ),
    ("Ctrl-C", "Quit"),
    ("", "Press any key to close"),
];

/// What each key does, shown with `?`.
pub const HELP: [(&str, &str); 18] = [
    ("Tab / Shift-Tab", "Next pane / previous pane"),
    ("Enter", "Open the folder or message"),
    ("→ ←  l h", "Expand or enter a folder, collapse it or go up"),
    ("*", "Add the folder to Favorites or take it out"),
    ("↑ ↓  j k", "Move, or scroll the message"),
    ("PgUp PgDn Ctrl-B Ctrl-F", "Move or scroll a page"),
    ("Ctrl-U Ctrl-D", "Move or scroll half a page"),
    ("g g  G  Home End", "Jump to the first or last"),
    ("z", "Zoom the pane, or show every pane again"),
    ("b", "Hide or show the folders"),
    (
        "Space, Ctrl-A",
        "Mark a message, mark them all (Esc clears)",
    ),
    ("d", "Move the marked or selected to Deleted Items"),
    ("D", "Delete the marked or selected for good"),
    (
        "c  r  R  f",
        "Write a message, reply, reply to all, forward",
    ),
    ("F5  Ctrl-R", "Sync every folder now"),
    ("?", "Show this help"),
    ("q  Ctrl-C", "Quit"),
    ("", "Press any key to close"),
];

/// Width of the key column in the help.
const HELP_KEYS: usize = 25;

/// Width of the help dialog, borders included.
pub const HELP_WIDTH: u16 = 76;

fn draw_help(frame: &mut Frame, help: &[(&str, &str)]) {
    let lines: Vec<Line> = help
        .iter()
        .map(|(keys, action)| {
            Line::from(vec![
                Span::styled(format!("{keys:<HELP_KEYS$}"), Style::new().fg(Color::Cyan)),
                Span::raw(*action),
            ])
        })
        .collect();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(HELP_WIDTH), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title("Keys")),
        area,
    );
}

/// Characters the longest help line takes, to check it fits.
pub fn help_line_width(help: &[(&str, &str)]) -> usize {
    help.iter()
        .map(|(_, action)| HELP_KEYS + action.chars().count())
        .max()
        .unwrap_or(0)
}

/// A dialog asking whether to delete the marked or selected messages.
fn draw_confirm_delete(frame: &mut Frame, count: usize, permanent: bool) {
    let messages = match count {
        1 => "1 message".to_owned(),
        n => format!("{n} messages"),
    };
    let mut lines = if permanent {
        vec![
            Line::raw(format!("Permanently delete {messages}?")),
            Line::raw("This cannot be undone."),
        ]
    } else {
        vec![Line::raw(format!("Move {messages} to Deleted Items?"))]
    };
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("y / Enter", Style::new().fg(Color::Red)),
        Span::raw(" delete   "),
        Span::styled("Esc", Style::new().fg(Color::Cyan)),
        Span::raw(" or any other key cancel"),
    ]));
    let title = if permanent {
        "Delete permanently"
    } else {
        "Delete"
    };
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(52), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(title)
                .border_style(Style::new().fg(Color::Red)),
        ),
        area,
    );
}

fn pane_block(app: &AppState, pane: Pane, title: &str) -> Block<'static> {
    section_block(app.focus == pane, title)
}

fn section_block(focused: bool, title: &str) -> Block<'static> {
    let border = if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    };
    Block::bordered()
        .border_type(border)
        .title(format!(" {title} "))
}

/// The rows shown in the favorites and below them, as indexes into
/// `folders`.
fn folder_sections(app: &AppState) -> (Vec<usize>, Vec<usize>) {
    let favorites = app.favorite_count();
    app.visible_folders()
        .into_iter()
        .partition(|&i| i < favorites)
}

fn draw_folders(frame: &mut Frame, app: &AppState, favorites: Rect, folders: Rect) {
    let depths = folder_depths(&app.folders);
    let (favorite_rows, folder_rows) = folder_sections(app);
    let in_favorites = app.selected_folder < app.favorite_count();
    let focused = app.focus == Pane::Folders;
    let favorites_block = section_block(focused && in_favorites, "Favorites");
    if favorite_rows.is_empty() {
        let hint = Line::styled(
            "* adds the selected folder",
            Style::new().fg(Color::DarkGray),
        );
        frame.render_widget(Paragraph::new(hint).block(favorites_block), favorites);
    } else {
        draw_folder_rows(
            frame,
            app,
            &depths,
            &favorite_rows,
            favorites_block,
            favorites,
        );
    }
    let title = match (app.accounts.len() > 1, app.sync.running) {
        (true, true) => "Accounts ⟳",
        (true, false) => "Accounts",
        (false, true) => "Folders ⟳",
        (false, false) => "Folders",
    };
    let block = section_block(focused && !in_favorites, title);
    draw_folder_rows(frame, app, &depths, &folder_rows, block, folders);
}

fn draw_folder_rows(
    frame: &mut Frame,
    app: &AppState,
    depths: &[usize],
    rows: &[usize],
    block: Block<'static>,
    area: Rect,
) {
    let items = rows.iter().map(|&index| {
        let folder = &app.folders[index];
        let marker = match (app.has_subfolders(index), app.is_expanded(index)) {
            (false, _) => "  ",
            (true, true) => "▾ ",
            (true, false) => "▸ ",
        };
        let mut spans = vec![
            Span::raw("  ".repeat(depths[index])),
            Span::styled(marker, Style::new().fg(Color::DarkGray)),
            if is_account_row(folder) {
                Span::styled(
                    shown(&folder.name),
                    Style::new().add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(shown(&folder.name))
            },
        ];
        let unread = app.unread_shown(index);
        if unread > 0 {
            spans.push(Span::raw(format!(" ({unread})")));
        }
        ListItem::new(Line::from(spans))
    });
    let list = List::new(items).block(block).highlight_symbol("> ");
    let selected = rows.iter().position(|&i| i == app.selected_folder);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_messages(frame: &mut Frame, app: &AppState, area: Rect) {
    let query = app.results_query();
    let title = match query {
        Some(query) => format!("Search: {} ({})", shown(query), app.messages.len()),
        None => "Messages".to_owned(),
    };
    let title = match app.marked.len() {
        0 => title,
        marked => format!("{title} ({marked} marked)"),
    };
    let block = pane_block(app, Pane::Messages, &title);
    if app.messages.is_empty() {
        let empty = if query.is_some() {
            "Nothing found"
        } else {
            "No messages"
        };
        frame.render_widget(Paragraph::new(empty).block(block), area);
        return;
    }
    let items = app.messages.iter().map(|m| {
        let style = if m.is_read {
            Style::new()
        } else {
            Style::new().add_modifier(Modifier::BOLD)
        };
        let mark = if app.marked.contains(&m.id) {
            Span::styled("● ", Style::new().fg(Color::Yellow))
        } else {
            Span::raw("")
        };
        let mut second = vec![Span::raw("  ")];
        if query.is_some() {
            let place = Style::new().fg(Color::DarkGray);
            second.push(Span::styled(format!("{} · ", found_in(app, m)), place));
        }
        second.push(Span::raw(shown(&m.subject)));
        ListItem::new(vec![
            Line::from(vec![mark, Span::styled(shown(&m.from_name), style)]),
            Line::from(second),
        ])
    });
    let list = List::new(items).block(block).highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(app.selected_message));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Where a search result is: its folder, and its account when there are
/// several.
fn found_in(app: &AppState, message: &Message) -> String {
    let folder = app
        .folders
        .iter()
        .find(|f| f.id == message.folder_id && !is_favorite_row(f))
        .map_or("?", |f| f.name.as_str());
    let account = app
        .accounts
        .get(app.message_account(&message.id))
        .filter(|_| app.accounts.len() > 1);
    match account {
        Some(address) => format!("{}, {address}", shown(folder)),
        None => shown(folder),
    }
}

fn draw_reader(frame: &mut Frame, app: &AppState, area: Rect) {
    let block = pane_block(app, Pane::Reader, "Reading");
    let Some(text) = reader_paragraph(app) else {
        frame.render_widget(block, area);
        return;
    };
    frame.render_widget(
        text.block(block)
            .scroll((app.reader_scroll.try_into().unwrap_or(u16::MAX), 0)),
        area,
    );
}

/// The selected message's headers and body, wrapped to the pane.
fn reader_paragraph(app: &AppState) -> Option<Paragraph<'static>> {
    let m = app.messages.get(app.selected_message)?;
    let mut text = vec![
        Line::raw(format!(
            "From: {} <{}>",
            shown(&m.from_name),
            shown(&m.from_addr)
        )),
        Line::raw(format!("Subject: {}", shown(&m.subject))),
        Line::raw(""),
    ];
    // The preview stands in until core has loaded the body.
    let body = app.body.as_deref().unwrap_or(&m.preview);
    text.extend(shown(body).lines().map(|l| Line::raw(l.to_owned())));
    Some(Paragraph::new(text).wrap(Wrap { trim: false }))
}

/// The pane at a point of `area`, and the folder or message row there.
pub fn hit(area: Rect, app: &AppState, column: u16, row: u16) -> Option<Hit> {
    let panes = panes(area, app);
    let at = Position::new(column, row);
    let (pane, rect) = [
        (Pane::Folders, panes.favorites),
        (Pane::Folders, panes.folders),
        (Pane::Messages, panes.messages),
        (Pane::Reader, panes.reader),
    ]
    .into_iter()
    .find(|(_, rect)| rect.contains(at))?;
    let inner = Block::bordered().inner(rect);
    let line = inner.contains(at).then(|| usize::from(at.y - inner.y));
    let rows = usize::from(inner.height);
    let item = match pane {
        Pane::Folders => line.and_then(|line| {
            // Rows are the folders shown; the hit is the folder's index.
            let (favorites, folders) = folder_sections(app);
            let section = if rect == panes.favorites {
                favorites
            } else {
                folders
            };
            // A section without the selection shows its first rows.
            let selected = section.iter().position(|&i| i == app.selected_folder);
            let offset = selected.map_or(0, |selected| list_offset(selected, rows));
            section.get(offset + line).copied()
        }),
        Pane::Messages => line
            .map(|line| {
                list_offset(app.selected_message, rows / MESSAGE_ROWS) + line / MESSAGE_ROWS
            })
            .filter(|&item| item < app.messages.len()),
        Pane::Reader => None,
    };
    Some(Hit { pane, item })
}

/// The first row a list shows. Lists are drawn from a fresh state each time,
/// so ratatui scrolls only as far as it takes to show the selected row.
fn list_offset(selected: usize, visible: usize) -> usize {
    selected.saturating_sub(visible.saturating_sub(1))
}

/// What each pane can show in `area`, for paging and scroll limits.
pub fn viewport(area: Rect, app: &AppState) -> Viewport {
    let panes = panes(area, app);
    // Every pane has a one-cell border all round.
    let inner = |r: Rect| {
        (
            r.width.saturating_sub(2),
            usize::from(r.height.saturating_sub(2)),
        )
    };
    let (_, folders) = inner(panes.folders);
    let (_, messages) = inner(panes.messages);
    let (reader_width, reader) = inner(panes.reader);
    Viewport {
        folders,
        messages: messages / MESSAGE_ROWS,
        reader,
        reader_lines: reader_paragraph(app).map_or(0, |p| p.line_count(reader_width)),
        compose: app.compose.as_ref().map_or(0, |compose| {
            usize::from(compose_areas(panes.compose, compose).body.height)
        }),
    }
}
