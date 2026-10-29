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
    app::{AppState, Confirm, Hit, PACE_WINDOW, Pane, Status, Viewport},
    compose::{Compose, Field},
    dialog::Dialog,
    editor::{Editor, Pos},
    folder_edit::NamePrompt,
    groups::MessageRow,
    input::TextInput,
    mover::MovePicker,
    older::LOADING_OLDER,
    rows::{is_account_row, is_favorite_row, is_unread_row},
    signature::SignatureEditor,
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
    /// The folders with unread mail, between the favorites and the
    /// accounts' folders.
    unread: Rect,
    folders: Rect,
    messages: Rect,
    reader: Rect,
    /// The message or signature being written, which takes the place of
    /// the panes.
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
    if app.compose.is_some() || app.signature.is_some() {
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
        [panes.favorites, panes.unread, panes.folders] = split_folders(panes.folders, app);
    }
    panes
}

/// Splits the folder pane into the favorites and the unread section, each
/// as tall as it needs up to a third of the pane, and the accounts' folders
/// below them.
fn split_folders(area: Rect, app: &AppState) -> [Rect; 3] {
    // An empty box still has a row, for its hint.
    let height = |rows: usize| {
        let rows = u16::try_from(rows.max(1)).unwrap_or(u16::MAX);
        rows.saturating_add(2).min(area.height / 3)
    };
    let unread_rows = app
        .visible_folders()
        .into_iter()
        .filter(|&i| is_unread_row(&app.folders[i]))
        .count();
    Layout::vertical([
        Constraint::Length(height(app.favorite_count())),
        Constraint::Length(height(unread_rows)),
        Constraint::Fill(1),
    ])
    .areas(area)
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let Panes {
        favorites,
        unread,
        folders,
        messages,
        reader,
        compose,
        status,
    } = panes(frame.area(), app);

    if !folders.is_empty() {
        draw_folders(frame, app, [favorites, unread, folders]);
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
    if let Some(editor) = &app.signature {
        draw_signature(frame, app, editor, compose);
    }
    let (line, cursor) = status_line(app, status.width);
    frame.render_widget(line, status);
    if let Some(column) = cursor {
        let x = status.x.saturating_add(column);
        frame.set_cursor_position((x.min(status.right().saturating_sub(1)), status.y));
    }

    match &app.dialog {
        Some(Dialog::Help) => {
            let help: &[(&str, &str)] = match (&app.compose, &app.signature) {
                (Some(_), _) => &COMPOSE_HELP,
                (None, Some(_)) => &SIGNATURE_HELP,
                (None, None) => &HELP,
            };
            draw_help(frame, help);
        }
        Some(Dialog::Blocked(said)) => draw_blocked(frame, said),
        Some(Dialog::Confirm(confirm)) => draw_confirm(frame, confirm),
        Some(Dialog::Name(prompt)) => draw_name_prompt(frame, prompt),
        Some(Dialog::Move(mover)) => draw_move_picker(frame, mover),
        None => {}
    }
}

fn draw_confirm(frame: &mut Frame, confirm: &Confirm) {
    match confirm {
        Confirm::Delete { ids, permanent } => {
            let messages = match ids.len() {
                1 => "1 message".to_owned(),
                n => format!("{n} messages"),
            };
            draw_confirm_delete(frame, &messages, *permanent);
        }
        Confirm::DeleteFolder {
            name, permanent, ..
        } => {
            let folder = format!("{} and everything in it", shown(name));
            draw_confirm_delete(frame, &folder, *permanent);
        }
        Confirm::Discard => draw_confirm_discard(
            frame,
            "Discard message",
            "Close the message and lose what you wrote?",
        ),
        Confirm::DiscardSignature => draw_confirm_discard(
            frame,
            "Discard signature",
            "Close the signature and lose your changes?",
        ),
        Confirm::SendAnyway { sent_lately, .. } => draw_confirm_burst(frame, *sent_lately),
    }
}

/// Folders the move dialog lists at most at once.
const MOVE_ROWS: usize = 12;

/// A dialog picking the folder to move mail to: the filter typed, then the
/// folders it leaves, in a tree.
fn draw_move_picker(frame: &mut Frame, mover: &MovePicker) {
    let choices = mover.choices();
    // Borders, the filter, the gap below it and the keys take 5 lines.
    let fits = usize::from(frame.area().height).saturating_sub(5);
    let visible = choices.len().clamp(1, MOVE_ROWS).min(fits.max(1));
    let first = list_offset(mover.selected, visible);
    // Room for the filter and the cursor after it, inside the borders.
    let room = usize::from(PROMPT_WIDTH - 3).saturating_sub(FILTER_LABEL.len());
    let (filter, column) = typed(&mover.filter, room);
    let mut lines = vec![Line::raw(format!("{FILTER_LABEL}{filter}")), Line::raw("")];
    if choices.is_empty() {
        lines.push(Line::styled(
            "No folder matches",
            Style::new().fg(Color::DarkGray),
        ));
    }
    for (row, &index) in choices.iter().enumerate().skip(first).take(visible) {
        let (folder, depth) = &mover.folders[index];
        let picked = row == mover.selected;
        let text = format!(
            "{}{}{}",
            if picked { "> " } else { "  " },
            "  ".repeat(*depth),
            shown(&folder.name)
        );
        let style = if picked {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(Line::styled(text, style));
    }
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::new().fg(Color::Yellow)),
        Span::raw(" move   "),
        Span::styled("↑ ↓", Style::new().fg(Color::Cyan)),
        Span::raw(" pick   "),
        Span::styled("Esc", Style::new().fg(Color::Cyan)),
        Span::raw(" cancel"),
    ]));
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(PROMPT_WIDTH), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(mover.title())
                .border_style(Style::new().fg(Color::Yellow)),
        ),
        area,
    );
    let label = u16::try_from(FILTER_LABEL.len()).unwrap_or(u16::MAX);
    let x = area.x.saturating_add(1 + label).saturating_add(column);
    frame.set_cursor_position((x.min(area.right().saturating_sub(2)), area.y + 1));
}

/// What the move dialog's filter is labelled with.
const FILTER_LABEL: &str = "Folder: ";

/// Width of the dialog asking a folder's name, borders included.
const PROMPT_WIDTH: u16 = 52;

/// A dialog asking the name of a folder to make, or a folder's new name.
fn draw_name_prompt(frame: &mut Frame, prompt: &NamePrompt) {
    // Room for the name and the cursor after it, inside the borders.
    let room = usize::from(PROMPT_WIDTH - 3);
    let (typed, column) = typed(&prompt.name, room);
    let lines = vec![
        Line::raw(typed),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Enter", Style::new().fg(Color::Yellow)),
            Span::raw(" save   "),
            Span::styled("Esc", Style::new().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ]),
    ];
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX) + 2;
    let area = frame
        .area()
        .centered(Constraint::Length(PROMPT_WIDTH), Constraint::Length(height));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(shown(&prompt.title()))
                .border_style(Style::new().fg(Color::Yellow)),
        ),
        area,
    );
    let x = area.x.saturating_add(1).saturating_add(column);
    frame.set_cursor_position((x.min(area.right().saturating_sub(2)), area.y + 1));
}

/// The part of `input` shown in `room` characters, made safe to draw, and
/// the column of its cursor.
fn typed(input: &TextInput, room: usize) -> (String, u16) {
    let (text, cursor) = input.view(room);
    let before: String = text.chars().take(cursor).collect();
    let column = u16::try_from(Line::raw(shown(&before)).width()).unwrap_or(u16::MAX);
    (shown(&text), column)
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

/// The status line `width` columns wide, and the column of the cursor in it
/// while a search is typed.
fn status_line(app: &AppState, width: u16) -> (Line<'static>, Option<u16>) {
    let mut spans = Vec::new();
    let editing = match (&app.compose, &app.signature) {
        (Some(compose), _) if compose.field == Field::Body => Some(&compose.body),
        (None, Some(editor)) => Some(&editor.body),
        _ => None,
    };
    let picking = app.viewer.mode() != VimMode::Normal;
    let mode = match editing {
        Some(body) => Some(body.vim.mode),
        None => (picking || app.focus == Pane::Reader).then(|| app.viewer.mode()),
    };
    if let Some(mode) = mode {
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
        let used = Line::from(spans.clone()).width();
        let (query, column) = typed(&search.query, usize::from(width).saturating_sub(used));
        spans.push(Span::raw(query));
        let used = u16::try_from(used).unwrap_or(u16::MAX);
        return (Line::from(spans), Some(used.saturating_add(column)));
    }
    spans.push(match &app.status {
        Some(Status::Error(error)) => {
            Span::styled(format!(" {error}"), Style::new().fg(Color::Red))
        }
        Some(Status::Info(info) | Status::Busy(info)) => Span::raw(format!(" {info}")),
        None if app.compose.is_some() => {
            Span::raw(" Ctrl-S send · Tab next field · Esc close · ? help")
        }
        None if app.signature.is_some() => Span::raw(" Ctrl-S save · Esc close · ? help"),
        None if picking => Span::raw(" y copy · Esc cancel"),
        None if app.focus == Pane::Reader => {
            Span::raw(" v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit")
        }
        None => Span::raw(
            " ? help · Tab switch pane · Enter open · / search · c write · r reply · d delete · q quit",
        ),
    });
    (Line::from(spans), None)
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
        let (text, column) = if focused {
            input.view(width)
        } else {
            (input.text().to_owned(), 0)
        };
        let line = Line::from(vec![
            Span::styled(
                format!("{label:<width$}", width = usize::from(LABEL_WIDTH)),
                label_style,
            ),
            Span::raw(text),
        ]);
        frame.render_widget(line, row);
        if focused {
            let column = u16::try_from(column).unwrap_or(u16::MAX);
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
    draw_body(
        frame,
        &compose.body,
        compose.field == Field::Body,
        areas.body,
    );
}

/// Where the note and the text of the signature being written go.
fn signature_areas(area: Rect) -> [Rect; 2] {
    let inner = Block::bordered().inner(area);
    Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner)
}

fn draw_signature(frame: &mut Frame, app: &AppState, editor: &SignatureEditor, area: Rect) {
    let address = app.accounts.get(editor.account).map_or("", String::as_str);
    let title = match address {
        "" => " Signature ".to_owned(),
        address => format!(" Signature — {address} "),
    };
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Thick)
            .title(title),
        area,
    );
    let [note, body] = signature_areas(area);
    frame.render_widget(
        Line::styled(
            "Put below each message you write from this account",
            Style::new().fg(Color::DarkGray),
        ),
        note,
    );
    draw_body(frame, &editor.body, true, body);
}

/// Text being written, scrolled so the cursor is in view. The cursor shows
/// when `focused`.
fn draw_body(frame: &mut Frame, editor: &Editor, focused: bool, area: Rect) {
    let height = usize::from(area.height);
    let width = usize::from(area.width).max(1);
    let (row, column) = editor.cursor();
    let top = editor.scroll(height);
    let left = column.saturating_sub(width - 1);
    let selection = editor.vim.selection((row, column));
    let lines: Vec<Line> = editor
        .lines()
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(index, line)| picked_line(line, index, left, selection))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
    if focused {
        let x = u16::try_from(column - left).unwrap_or(u16::MAX);
        let y = u16::try_from(row - top).unwrap_or(u16::MAX);
        frame.set_cursor_position((area.x + x, area.y + y));
    }
}

/// Line `index` from its character `left` on, with the characters in a
/// visual mode's `selection` reversed. An empty line in it shows one
/// reversed cell.
fn picked_line(
    line: &str,
    index: usize,
    left: usize,
    selection: Option<(Pos, Pos)>,
) -> Line<'static> {
    let chars = line.chars().enumerate().skip(left);
    if selection.is_none_or(|(first, last)| index < first.0 || last.0 < index) {
        return Line::raw(chars.map(|(_, c)| c).collect::<String>());
    }
    let picked = Style::new().add_modifier(Modifier::REVERSED);
    let selected =
        |at| selection.is_some_and(|(first, last)| first <= (index, at) && (index, at) <= last);
    if line.is_empty() && selected(0) {
        return Line::styled(" ", picked);
    }
    let spans = chars.map(|(at, c)| {
        if selected(at) {
            Span::styled(c.to_string(), picked)
        } else {
            Span::raw(c.to_string())
        }
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// A dialog asking whether to close the message or signature being written.
fn draw_confirm_discard(frame: &mut Frame, title: &str, question: &str) {
    let lines = vec![
        Line::raw(question.to_owned()),
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
                .title(title.to_owned())
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

/// What each key does while writing a signature, shown with `?` in normal mode.
pub const SIGNATURE_HELP: [(&str, &str); 9] = [
    ("i a I A o O", "Type, insert mode"),
    ("Esc", "Back to normal mode"),
    ("h j k l w b e 0 $ gg G", "Move"),
    ("dd cw x p u Ctrl-R v V", "Edit vim style"),
    ("Ctrl-S", "Save"),
    (
        "Esc in normal mode",
        "Close, asking first if anything changed",
    ),
    ("", "An empty signature adds nothing to messages"),
    ("Ctrl-C", "Quit"),
    ("", "Press any key to close"),
];

/// What each key does, shown with `?`.
pub const HELP: [(&str, &str); 22] = [
    ("Tab / Shift-Tab", "Next pane / previous pane"),
    ("Enter", "Open the folder or message, fold a date group"),
    (
        "→ ←  l h",
        "Unfold/fold folder or group; → ← next/prev mail",
    ),
    ("*", "Add the folder to Favorites or take it out"),
    ("↑ ↓  j k", "Move, or scroll the message"),
    ("PgUp PgDn Ctrl-B Ctrl-F", "Move or scroll a page"),
    ("Ctrl-U Ctrl-D", "Move or scroll half a page"),
    ("g g  G  Home End", "Jump to the first or last"),
    (
        "h j k l w b e 0 $",
        "Move the cursor in the message, as Vim does",
    ),
    (
        "v V then y, or yy",
        "Pick text or lines from the cursor, copy them",
    ),
    ("z  b", "Zoom the pane or show all, hide the folders"),
    (
        "Space, Ctrl-A",
        "Mark a message, mark them all (Esc clears)",
    ),
    ("d", "Move marked, selected or folder to Deleted Items"),
    ("D", "Delete marked, selected or folder for good"),
    ("m", "Move marked or selected mail to a folder"),
    ("a  A  F2", "New subfolder, new top folder, rename folder"),
    (
        "c  r  R  f",
        "Write a message, reply, reply to all, forward",
    ),
    ("F5  Ctrl-R", "Sync every folder now"),
    ("/", "Search every folder (Esc goes back)"),
    ("S", "Write this account's signature"),
    ("?  q  Ctrl-C", "Show this help, quit"),
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

/// A dialog asking whether to delete `what`: messages, or a folder.
fn draw_confirm_delete(frame: &mut Frame, what: &str, permanent: bool) {
    let mut lines = if permanent {
        vec![
            Line::raw(format!("Permanently delete {what}?")),
            Line::raw("This cannot be undone."),
        ]
    } else {
        vec![Line::raw(format!("Move {what} to Deleted Items?"))]
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
    // Wide enough for a long folder name, borders and a space each side.
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest + 4).unwrap_or(u16::MAX).max(52);
    let area = frame
        .area()
        .centered(Constraint::Length(width), Constraint::Length(height));
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

/// The rows shown in the favorites, in the unread section and below them,
/// as indexes into `folders`.
fn folder_sections(app: &AppState) -> [Vec<usize>; 3] {
    let favorites = app.favorite_count();
    let unread = favorites + app.unread_count();
    let mut sections: [Vec<usize>; 3] = Default::default();
    for i in app.visible_folders() {
        let section = if i < favorites {
            0
        } else if i < unread {
            1
        } else {
            2
        };
        sections[section].push(i);
    }
    sections
}

/// Draws the favorites, the unread section and the accounts' folders in
/// the areas given, in that order.
fn draw_folders(frame: &mut Frame, app: &AppState, areas: [Rect; 3]) {
    let depths = folder_depths(&app.folders);
    let favorites = app.favorite_count();
    let unread = favorites + app.unread_count();
    let selected = app.selected_folder;
    let section = if selected < favorites {
        0
    } else if selected < unread {
        1
    } else {
        2
    };
    let focused = app.focus == Pane::Folders;
    let accounts = match (app.accounts.len() > 1, app.sync.running) {
        (true, true) => "Accounts ⟳",
        (true, false) => "Accounts",
        (false, true) => "Folders ⟳",
        (false, false) => "Folders",
    };
    let titles = ["Favorites", "Unread", accounts];
    let hints = ["* adds the selected folder", "No unread mail", ""];
    for (index, rows) in folder_sections(app).iter().enumerate() {
        let block = section_block(focused && section == index, titles[index]);
        if rows.is_empty() && index < 2 {
            let hint = Line::styled(hints[index], Style::new().fg(Color::DarkGray));
            frame.render_widget(Paragraph::new(hint).block(block), areas[index]);
        } else {
            draw_folder_rows(frame, app, &depths, rows, block, areas[index]);
        }
    }
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
        } else if app.loading_older.is_some() {
            LOADING_OLDER
        } else {
            "No messages"
        };
        frame.render_widget(Paragraph::new(empty).block(block), area);
        return;
    }
    let rows = app.message_rows();
    let items = rows.iter().map(|row| {
        let m = match *row {
            MessageRow::Header {
                group,
                count,
                collapsed,
            } => {
                let fold = if collapsed { "▸" } else { "▾" };
                let style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
                let title = format!("{fold} {} ({count})", group.title());
                return ListItem::new(Line::styled(title, style));
            }
            MessageRow::Message(index) => &app.messages[index],
        };
        let style = if m.is_read {
            Style::new().fg(Color::DarkGray)
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
        second.push(Span::styled(shown(&m.subject), style));
        ListItem::new(vec![
            Line::from(vec![mark, Span::styled(shown(&m.from_name), style)]),
            Line::from(second),
        ])
    });
    let list = List::new(items).block(block).highlight_symbol("> ");
    let mut state = ListState::default().with_selected(app.selected_row(&rows));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Where a search result is: its folder, and its account when there are
/// several.
fn found_in(app: &AppState, message: &Message) -> String {
    let folder = app
        .folders
        .iter()
        .find(|f| f.id == message.folder_id && !is_favorite_row(f) && !is_unread_row(f))
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
    // The cursor shows as a selection of its own character.
    let cursor = app.viewer.cursor();
    let focused = app.focus == Pane::Reader;
    let selection = app
        .viewer
        .selection()
        .or_else(|| focused.then_some((cursor, cursor)));
    let text: Vec<Line> = app
        .reader_text()?
        .iter()
        .enumerate()
        .map(|(index, line)| picked_line(line, index, 0, selection))
        .collect();
    Some(Paragraph::new(text).wrap(Wrap { trim: false }))
}

/// The pane at a point of `area`, and the folder or message row there.
pub fn hit(area: Rect, app: &AppState, column: u16, row: u16) -> Option<Hit> {
    let panes = panes(area, app);
    let at = Position::new(column, row);
    let (pane, rect) = [
        (Pane::Folders, panes.favorites),
        (Pane::Folders, panes.unread),
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
            let [favorites, unread, folders] = folder_sections(app);
            let section = if rect == panes.favorites {
                favorites
            } else if rect == panes.unread {
                unread
            } else {
                folders
            };
            // A section without the selection shows its first rows.
            let selected = section.iter().position(|&i| i == app.selected_folder);
            let offset = selected.map_or(0, |selected| list_offset(selected, rows));
            section.get(offset + line).copied()
        }),
        Pane::Messages => line.and_then(|line| {
            // Rows are the headers and messages shown; the hit is the row's index.
            let shown = app.message_rows();
            let heights: Vec<usize> = shown.iter().map(row_height).collect();
            let selected = app.selected_row(&shown).unwrap_or(0);
            row_at(&heights, rows_offset(&heights, selected, rows), line)
        }),
        Pane::Reader => None,
    };
    Some(Hit { pane, item })
}

/// The first row a list shows. Lists are drawn from a fresh state each time,
/// so ratatui scrolls only as far as it takes to show the selected row.
fn list_offset(selected: usize, visible: usize) -> usize {
    selected.saturating_sub(visible.saturating_sub(1))
}

/// Lines a row of the message list takes.
fn row_height(row: &MessageRow) -> usize {
    match row {
        MessageRow::Header { .. } => 1,
        MessageRow::Message(_) => MESSAGE_ROWS,
    }
}

/// The first row a list of rows `heights` tall shows in `height` lines,
/// drawn from a fresh state: as many rows as fit, ending with the selected.
fn rows_offset(heights: &[usize], selected: usize, height: usize) -> usize {
    let Some(&tallest) = heights.get(selected) else {
        return 0;
    };
    let mut first = selected;
    let mut used = tallest;
    while first > 0 && used + heights[first - 1] <= height {
        first -= 1;
        used += heights[first];
    }
    first
}

/// The row at `line` of a list of rows `heights` tall showing from `first`.
fn row_at(heights: &[usize], first: usize, line: usize) -> Option<usize> {
    let mut top = 0;
    for (row, &height) in heights.iter().enumerate().skip(first) {
        if line < top + height {
            return Some(row);
        }
        top += height;
    }
    None
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
        reader_width,
        compose: match (&app.compose, &app.signature) {
            (Some(compose), _) => usize::from(compose_areas(panes.compose, compose).body.height),
            (None, Some(_)) => usize::from(signature_areas(panes.compose)[1].height),
            (None, None) => 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_from_the_top_while_the_selected_one_fits() {
        assert_eq!(rows_offset(&[1, 2, 2, 1, 2], 2, 6), 0);
    }

    #[test]
    fn rows_scroll_until_the_selected_one_is_at_the_bottom() {
        // A header then messages: the fourth row ends on line 7.
        assert_eq!(rows_offset(&[1, 2, 2, 2], 3, 4), 2);
        assert_eq!(rows_offset(&[1, 2, 2, 1, 2], 4, 5), 2);
    }

    #[test]
    fn a_line_hits_the_row_drawn_on_it() {
        let heights = [1, 2, 2];
        assert_eq!(row_at(&heights, 0, 0), Some(0), "the header");
        assert_eq!(row_at(&heights, 0, 1), Some(1));
        assert_eq!(row_at(&heights, 0, 2), Some(1));
        assert_eq!(row_at(&heights, 0, 3), Some(2));
        assert_eq!(row_at(&heights, 0, 5), None, "below the last row");
        assert_eq!(row_at(&heights, 1, 2), Some(2), "scrolled past the header");
    }
}
