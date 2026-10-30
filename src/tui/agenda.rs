//! The calendar, listed day by day from a week ago to three months ahead.

use jiff::{civil::Date, tz::TimeZone};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::model::Appointment;

/// One line of the agenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// The heading of a day with appointments.
    Day(Date),
    /// The appointment at this index of [`Agenda::appointments`].
    Appointment(usize),
}

/// One account's calendar, taking over the screen.
#[derive(Debug)]
pub struct Agenda {
    /// The account whose calendar it is.
    pub account: usize,
    pub address: String,
    appointments: Vec<Appointment>,
    /// Index into `appointments`.
    selected: usize,
    /// Whether the selected appointment is shown in full.
    pub details: bool,
    time_zone: TimeZone,
    today: Date,
}

impl Agenda {
    /// An empty agenda of `account`, at `address`, whose days are those of
    /// `time_zone`, starting from `today`.
    pub fn new(account: usize, address: String, time_zone: TimeZone, today: Date) -> Self {
        Self {
            account,
            address,
            appointments: vec![],
            selected: 0,
            details: false,
            time_zone,
            today,
        }
    }

    /// Shows `appointments`, soonest first. The selection stays on the same
    /// appointment if it is still there, or else goes to today's first.
    pub fn set_appointments(&mut self, appointments: Vec<Appointment>) {
        let was = self.selected().map(|a| a.id.clone());
        self.appointments = appointments;
        self.selected = match was.and_then(|id| self.appointments.iter().position(|a| a.id == id)) {
            Some(index) => index,
            None => self.first_from_today(),
        };
    }

    /// Today's first appointment, or the next after it, or else the last.
    fn first_from_today(&self) -> usize {
        self.appointments
            .iter()
            .position(|a| self.day_of(a) >= self.today)
            .unwrap_or(self.appointments.len().saturating_sub(1))
    }

    pub fn appointments(&self) -> &[Appointment] {
        &self.appointments
    }

    pub fn selected(&self) -> Option<&Appointment> {
        self.appointments.get(self.selected)
    }

    /// The day `appointment` is listed under: the day it starts.
    pub fn day_of(&self, appointment: &Appointment) -> Date {
        appointment.start.to_zoned(self.time_zone.clone()).date()
    }

    pub fn today(&self) -> Date {
        self.today
    }

    pub fn time_zone(&self) -> &TimeZone {
        &self.time_zone
    }

    /// The lines of the agenda: each day's heading, then its appointments.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut last_day = None;
        for (index, appointment) in self.appointments.iter().enumerate() {
            let day = self.day_of(appointment);
            if last_day != Some(day) {
                rows.push(Row::Day(day));
                last_day = Some(day);
            }
            rows.push(Row::Appointment(index));
        }
        rows
    }

    /// Moves the selection, or leaves it at either end.
    pub fn key(&mut self, key: KeyEvent) {
        let last = self.appointments.len().saturating_sub(1);
        self.selected = match key.code {
            KeyCode::Char('j') | KeyCode::Down => (self.selected + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.selected.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => 0,
            KeyCode::Char('G') | KeyCode::End => last,
            KeyCode::Char('t') => self.first_from_today(),
            _ => self.selected,
        };
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use crate::model::AppointmentId;

    use super::*;

    fn appointment(id: &str, start: &str) -> Appointment {
        let start: jiff::Timestamp = start.parse().unwrap();
        Appointment {
            id: AppointmentId(id.to_owned()),
            subject: id.to_owned(),
            start,
            end: start + jiff::SignedDuration::from_hours(1),
            all_day: false,
            location: String::new(),
            organizer: String::new(),
            attendees: vec![],
            preview: String::new(),
            is_organizer: true,
        }
    }

    /// An agenda on 2 November 2026 in UTC, of yesterday's, two of today's
    /// and one of tomorrow's appointments.
    fn agenda() -> Agenda {
        let mut agenda = Agenda::new(
            0,
            "me@example.com".to_owned(),
            TimeZone::UTC,
            date(2026, 11, 2),
        );
        agenda.set_appointments(vec![
            appointment("yesterday", "2026-11-01T09:00:00Z"),
            appointment("today 9", "2026-11-02T09:00:00Z"),
            appointment("today 14", "2026-11-02T14:00:00Z"),
            appointment("tomorrow", "2026-11-03T09:00:00Z"),
        ]);
        agenda
    }

    fn selected(agenda: &Agenda) -> &str {
        &agenda.selected().unwrap().id.0
    }

    fn press(agenda: &mut Agenda, code: KeyCode) {
        agenda.key(KeyEvent::from(code));
    }

    #[test]
    fn appointments_are_listed_under_the_day_they_start() {
        assert_eq!(
            agenda().rows(),
            [
                Row::Day(date(2026, 11, 1)),
                Row::Appointment(0),
                Row::Day(date(2026, 11, 2)),
                Row::Appointment(1),
                Row::Appointment(2),
                Row::Day(date(2026, 11, 3)),
                Row::Appointment(3),
            ]
        );
    }

    #[test]
    fn the_agenda_opens_on_todays_first_appointment() {
        assert_eq!(selected(&agenda()), "today 9");
    }

    #[test]
    fn with_nothing_from_today_on_the_last_appointment_is_selected() {
        let mut agenda = agenda();

        agenda.set_appointments(vec![
            appointment("older", "2026-10-30T09:00:00Z"),
            appointment("yesterday", "2026-11-01T09:00:00Z"),
        ]);

        assert_eq!(selected(&agenda), "yesterday");
    }

    #[test]
    fn j_and_k_move_between_appointments_over_the_days() {
        let mut agenda = agenda();

        press(&mut agenda, KeyCode::Char('j'));
        press(&mut agenda, KeyCode::Char('j'));
        assert_eq!(selected(&agenda), "tomorrow");
        press(&mut agenda, KeyCode::Char('j'));
        assert_eq!(selected(&agenda), "tomorrow");
        press(&mut agenda, KeyCode::Up);
        assert_eq!(selected(&agenda), "today 14");
    }

    #[test]
    fn g_and_shift_g_go_to_the_first_and_last_appointment_and_t_to_today() {
        let mut agenda = agenda();

        press(&mut agenda, KeyCode::Char('g'));
        assert_eq!(selected(&agenda), "yesterday");
        press(&mut agenda, KeyCode::Char('G'));
        assert_eq!(selected(&agenda), "tomorrow");
        press(&mut agenda, KeyCode::Char('t'));
        assert_eq!(selected(&agenda), "today 9");
    }

    #[test]
    fn a_refreshed_agenda_stays_on_the_selected_appointment() {
        let mut agenda = agenda();
        press(&mut agenda, KeyCode::Char('G'));

        let mut appointments = agenda.appointments().to_vec();
        appointments.insert(0, appointment("new", "2026-10-31T09:00:00Z"));
        agenda.set_appointments(appointments);

        assert_eq!(selected(&agenda), "tomorrow");
    }

    #[test]
    fn an_empty_agenda_selects_nothing() {
        let mut agenda = agenda();

        agenda.set_appointments(vec![]);
        press(&mut agenda, KeyCode::Char('j'));

        assert!(agenda.selected().is_none());
        assert!(agenda.rows().is_empty());
    }
}
