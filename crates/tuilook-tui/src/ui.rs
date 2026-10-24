//! Renders [`AppState`] into a frame. Pure: no IO, no state changes.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{AppState, Pane};

pub fn draw(frame: &mut Frame, app: &AppState) {
    let [main, status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    let [folders, messages, reader] = Layout::horizontal([
        Constraint::Percentage(20),
        Constraint::Percentage(40),
        Constraint::Percentage(40),
    ])
    .areas(main);

    draw_folders(frame, app, folders);
    draw_messages(frame, app, messages);
    draw_reader(frame, app, reader);
    frame.render_widget(
        Line::from(" q quit · Tab/h/l switch pane · j/k move · Enter open"),
        status,
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
    let items = app.folders.iter().map(|f| {
        let mut spans = vec![Span::raw(f.name.clone())];
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
    let block = pane_block(app, Pane::Messages, "Messages");
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
        ListItem::new(vec![
            Line::styled(m.from_name.clone(), style),
            Line::raw(format!("  {}", m.subject)),
        ])
    });
    let list = List::new(items).block(block).highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(app.selected_message));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_reader(frame: &mut Frame, app: &AppState, area: Rect) {
    let block = pane_block(app, Pane::Reader, "Reading");
    let Some(m) = app.messages.get(app.selected_message) else {
        frame.render_widget(block, area);
        return;
    };
    let text = vec![
        Line::raw(format!("From: {} <{}>", m.from_name, m.from_addr)),
        Line::raw(format!("Subject: {}", m.subject)),
        Line::raw(""),
        Line::raw(m.preview.clone()),
    ];
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        area,
    );
}
