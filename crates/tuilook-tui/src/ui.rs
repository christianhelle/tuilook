//! Renders [`AppState`] into a frame. Pure: no IO, no state changes.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use tuilook_model::{ComposeKind, folder_depths};

use crate::{
    app::{AppState, Confirm, Hit, Pane, Status, Viewport},
    compose::{Compose, Field},
    vim::VimMode,
};

/// Each message takes this many rows in the message list.
const MESSAGE_ROWS: usize = 2;

/// Where each pane goes. A pane that is not shown gets an empty area.
#[derive(Debug, Default)]
struct Panes {
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
    panes
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let Panes {
        folders,
        messages,
        reader,
        compose,
        status,
    } = panes(frame.area(), app);

    if !folders.is_empty() {
        draw_folders(frame, app, folders);
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
        None => {}
    }
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
    let [to, cc, subject, rule, note, body] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(note),
        Constraint::Fill(1),
    ])
    .areas(inner);
    ComposeAreas {
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
pub const HELP: [(&str, &str); 16] = [
    ("Tab / Shift-Tab  l h", "Next pane / previous pane"),
    ("Enter", "Open the folder or message"),
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
    let border = if app.focus == pane {
        BorderType::Thick
    } else {
        BorderType::Plain
    };
    Block::bordered()
        .border_type(border)
        .title(format!(" {title} "))
}

fn draw_folders(frame: &mut Frame, app: &AppState, area: Rect) {
    let depths = folder_depths(&app.folders);
    let items = app.folders.iter().zip(depths).map(|(f, depth)| {
        let mut spans = vec![Span::raw("  ".repeat(depth)), Span::raw(f.name.clone())];
        if f.unread > 0 {
            spans.push(Span::raw(format!(" ({})", f.unread)));
        }
        ListItem::new(Line::from(spans))
    });
    let list = List::new(items)
        .block(pane_block(
            app,
            Pane::Folders,
            if app.sync.running {
                "Folders ⟳"
            } else {
                "Folders"
            },
        ))
        .highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(app.selected_folder));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_messages(frame: &mut Frame, app: &AppState, area: Rect) {
    let title = match app.marked.len() {
        0 => "Messages".to_owned(),
        marked => format!("Messages ({marked} marked)"),
    };
    let block = pane_block(app, Pane::Messages, &title);
    if app.messages.is_empty() {
        frame.render_widget(Paragraph::new("No messages").block(block), area);
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
        ListItem::new(vec![
            Line::from(vec![mark, Span::styled(m.from_name.clone(), style)]),
            Line::raw(format!("  {}", m.subject)),
        ])
    });
    let list = List::new(items).block(block).highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(app.selected_message));
    frame.render_stateful_widget(list, area, &mut state);
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
        Line::raw(format!("From: {} <{}>", m.from_name, m.from_addr)),
        Line::raw(format!("Subject: {}", m.subject)),
        Line::raw(""),
    ];
    // The preview stands in until core has loaded the body.
    let body = app.body.as_deref().unwrap_or(&m.preview);
    text.extend(body.lines().map(|l| Line::raw(l.to_owned())));
    Some(Paragraph::new(text).wrap(Wrap { trim: false }))
}

/// The pane at a point of `area`, and the folder or message row there.
pub fn hit(area: Rect, app: &AppState, column: u16, row: u16) -> Option<Hit> {
    let panes = panes(area, app);
    let at = Position::new(column, row);
    let (pane, rect) = [
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
        Pane::Folders => line
            .map(|line| list_offset(app.selected_folder, rows) + line)
            .filter(|&item| item < app.folders.len()),
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
