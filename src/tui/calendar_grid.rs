//! The calendar laid out by the week or the month.

use jiff::civil::Date;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::Paragraph,
};

use crate::model::Appointment;
use crate::tui::{
    agenda::{Agenda, CalendarView},
    ui::shown,
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

/// Seven days side by side.
pub(crate) fn draw_week(frame: &mut Frame, agenda: &Agenda, area: Rect) {
    let columns: [Rect; 7] = Layout::horizontal([Constraint::Ratio(1, 7); 7]).areas(area);
    for (day, column) in agenda.visible_dates().into_iter().zip(columns) {
        draw_week_day(frame, agenda, day, column);
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
