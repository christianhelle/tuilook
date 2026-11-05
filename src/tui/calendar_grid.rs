//! The calendar laid out by the week or the month.

use jiff::civil::Date;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use crate::model::Appointment;
use crate::tui::{
    agenda::{Agenda, CalendarView},
    ui::{draw_appointment_details, shown, when},
};

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn reversed() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

/// What the calendar's title says is laid out, unless it is the agenda.
pub(crate) fn period(agenda: &Agenda) -> Option<String> {
    let (first, _) = agenda.visible_days()?;
    match agenda.view() {
        CalendarView::Agenda => None,
        CalendarView::Week => Some(format!("Week of {}", first.strftime("%-d %b %Y"))),
        CalendarView::Month => Some(agenda.day().strftime("%B %Y").to_string()),
    }
}

/// How a day is marked: the selected one stands out, then today.
fn day_style(agenda: &Agenda, day: Date) -> Style {
    let style = if day == agenda.today() {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::BOLD)
    };
    if day == agenda.day() {
        style.add_modifier(Modifier::REVERSED)
    } else {
        style
    }
}

/// Seven days side by side, with the selected appointment in full beside
/// them while asked for.
pub(crate) fn draw_week(frame: &mut Frame, agenda: &Agenda, area: Rect) {
    let [grid, details] = if agenda.details {
        Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)]).areas(area)
    } else {
        [area, Rect::default()]
    };
    let columns: [Rect; 7] = Layout::horizontal([Constraint::Ratio(1, 7); 7]).areas(grid);
    for (day, column) in agenda.visible_dates().into_iter().zip(columns) {
        draw_week_day(frame, agenda, day, column);
    }
    if let Some(appointment) = agenda.selected().filter(|_| agenda.details) {
        draw_appointment_details(frame, agenda, appointment, details);
    }
}

/// How `count` appointments fill `room` rows at two rows each: where to
/// start so that the `selected` one shows, how many to show, and whether a
/// row must say that others are hidden.
fn fitting(count: usize, room: usize, selected: Option<usize>) -> (usize, usize, bool) {
    if count <= room / 2 {
        return (0, count, false);
    }
    let fits = room.saturating_sub(1) / 2;
    let start = selected.map_or(0, |s| (s + 1).saturating_sub(fits));
    (start, fits, true)
}

/// The two rows of an appointment in `day`'s column.
fn week_rows(
    agenda: &Agenda,
    appointment: &Appointment,
    day: Date,
    selected: bool,
) -> [Line<'static>; 2] {
    let time = if appointment.all_day {
        "All day".to_owned()
    } else if agenda.day_of(appointment) == day {
        appointment
            .start
            .to_zoned(agenda.time_zone().clone())
            .strftime("%H:%M")
            .to_string()
    } else {
        // Started on an earlier day and still going.
        "…".to_owned()
    };
    let (time_style, subject_style) = if selected {
        (reversed(), reversed())
    } else {
        (dim(), Style::new())
    };
    [
        Line::styled(time, time_style),
        Line::styled(shown(&appointment.subject), subject_style),
    ]
}

fn draw_week_day(frame: &mut Frame, agenda: &Agenda, day: Date, column: Rect) {
    // A gap to the next column.
    let width = column.width.saturating_sub(1);
    let today = if day == agenda.today() { "•" } else { "" };
    let mut lines = vec![
        Line::styled(
            format!("{}{today}", day.strftime("%a %-d")),
            day_style(agenda, day),
        ),
        Line::styled("─".repeat(usize::from(width)), dim()),
    ];
    let appointments = agenda.appointments_on(day);
    let chosen = (day == agenda.day()).then(|| agenda.selected_on_day());
    let room = usize::from(column.height).saturating_sub(lines.len());
    let (start, fits, more) = fitting(appointments.len(), room, chosen);
    for (index, appointment) in appointments.iter().enumerate().skip(start).take(fits) {
        lines.extend(week_rows(agenda, appointment, day, chosen == Some(index)));
    }
    if more {
        let hidden = appointments.len() - fits;
        lines.push(Line::styled(format!("+{hidden} more"), dim()));
    }
    frame.render_widget(Paragraph::new(lines), Rect { width, ..column });
}

/// The weeks of a month, with the selected day's appointments beside them,
/// or the selected appointment in full while asked for.
pub(crate) fn draw_month(frame: &mut Frame, agenda: &Agenda, area: Rect) {
    let [grid, side] =
        Layout::horizontal([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)]).areas(area);
    draw_month_grid(frame, agenda, grid);
    match agenda.selected().filter(|_| agenda.details) {
        Some(appointment) => draw_appointment_details(frame, agenda, appointment, side),
        None => draw_month_side(frame, agenda, side),
    }
}

fn draw_month_grid(frame: &mut Frame, agenda: &Agenda, area: Rect) {
    let [names, weeks_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let columns =
        |row: Rect| -> [Rect; 7] { Layout::horizontal([Constraint::Ratio(1, 7); 7]).areas(row) };
    let weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for (name, cell) in weekdays.into_iter().zip(columns(names)) {
        frame.render_widget(Line::styled(name, dim()), cell);
    }
    let dates = agenda.visible_dates();
    let weeks = dates.chunks(7);
    let count = u32::try_from(weeks.len()).unwrap_or(1);
    let rows = Layout::vertical(vec![Constraint::Ratio(1, count); weeks.len()]).split(weeks_area);
    for (week, row) in weeks.zip(rows.iter()) {
        for (day, cell) in week.iter().zip(columns(*row)) {
            draw_month_day(frame, agenda, *day, cell);
        }
    }
}

/// A day of the month: its number, and as many of its appointments as fit,
/// or how many it has if none fit.
fn draw_month_day(frame: &mut Frame, agenda: &Agenda, day: Date, cell: Rect) {
    let width = cell.width.saturating_sub(1);
    let in_month = day.first_of_month() == agenda.day().first_of_month();
    let (number, text) = if in_month {
        (day_style(agenda, day), Style::new())
    } else {
        (dim(), dim())
    };
    let appointments = agenda.appointments_on(day);
    let room = usize::from(cell.height).saturating_sub(1);
    let mut first = vec![Span::styled(day.day().to_string(), number)];
    if room == 0 && !appointments.is_empty() {
        first.push(Span::styled(format!(" •{}", appointments.len()), text));
    }
    let mut lines = vec![Line::from(first)];
    let fits = if appointments.len() > room {
        room.saturating_sub(1)
    } else {
        room
    };
    for appointment in appointments.iter().take(fits) {
        lines.push(Line::styled(shown(&appointment.subject), text));
    }
    if room > 0 && appointments.len() > fits {
        lines.push(Line::styled(
            format!("+{}", appointments.len() - fits),
            dim(),
        ));
    }
    frame.render_widget(Paragraph::new(lines), Rect { width, ..cell });
}

/// The selected day's appointments, beside the month.
fn draw_month_side(frame: &mut Frame, agenda: &Agenda, area: Rect) {
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(dim())
        .padding(Padding::left(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [heading, list] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let day = agenda.day();
    let mut title = day.strftime("%a %-d %b").to_string();
    if day == agenda.today() {
        title.push_str(" · today");
    }
    frame.render_widget(
        Line::styled(
            title,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        heading,
    );
    let appointments = agenda.appointments_on(day);
    if appointments.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing. Press c to write one.")
                .style(dim())
                .wrap(Wrap { trim: true }),
            list,
        );
        return;
    }
    let items: Vec<ListItem> = appointments
        .iter()
        .map(|appointment| {
            ListItem::new(vec![
                Line::styled(when(agenda, appointment), dim()),
                Line::raw(shown(&appointment.subject)),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(agenda.selected_on_day()));
    frame.render_stateful_widget(
        List::new(items).highlight_style(reversed()),
        list,
        &mut state,
    );
}
