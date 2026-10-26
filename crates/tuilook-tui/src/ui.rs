//! Renders [`AppState`] into a frame. Pure: no IO, no state changes.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap},
};
use tuilook_model::folder_depths;

use crate::app::{AppState, Pane, Status, Viewport};

/// Each message takes this many rows in the message list.
const MESSAGE_ROWS: usize = 2;

/// Where each pane goes. A pane that is not shown gets an empty area.
#[derive(Debug, Default)]
struct Panes {
    folders: Rect,
    messages: Rect,
    reader: Rect,
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
    let status_line = match &app.status {
        Some(Status::Error(error)) => {
            Line::styled(format!(" {error}"), Style::new().fg(Color::Red))
        }
        Some(Status::Info(info)) => Line::raw(format!(" {info}")),
        None => Line::from(" q quit · Tab/h/l switch pane · j/k move · Enter open"),
    };
    frame.render_widget(status_line, status);
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
        .block(pane_block(app, Pane::Folders, "Folders"))
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
    }
}
