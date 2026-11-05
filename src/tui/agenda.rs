//! The calendar: listed day by day, or laid out by the week or the month.

use jiff::{Timestamp, ToSpan, civil::Date, tz::TimeZone};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde::{Deserialize, Serialize};

use crate::model::Appointment;

/// How the calendar is laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarView {
    /// Day by day, listing only the days with appointments.
    #[default]
    Agenda,
    /// The seven days of a week, Monday first, side by side.
    Week,
    /// The weeks of a month.
    Month,
}

/// What a key asks of the app, beyond moving about the agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Write a new appointment.
    New,
    Edit(Appointment),
    /// Delete the appointment, after asking.
    Delete(Appointment),
    /// Sync the calendar now.
    Refresh,
    Help,
    /// Show the next account's calendar instead.
    NextAccount,
    /// Show the previous account's calendar instead.
    PrevAccount,
}

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
    /// Index into `appointments`, the selection in [`CalendarView::Agenda`].
    selected: usize,
    /// Whether the selected appointment is shown in full.
    pub details: bool,
    time_zone: TimeZone,
    today: Date,
    view: CalendarView,
    /// The selected day in the week and the month.
    day: Date,
    /// The selected one of [`Agenda::appointments_on`] that day.
    on_day: usize,
}

impl Agenda {
    /// An empty calendar of `account`, at `address`, whose days are those of
    /// `time_zone`, laid out as `view` and starting from `today`.
    pub fn new(
        account: usize,
        address: String,
        time_zone: TimeZone,
        today: Date,
        view: CalendarView,
    ) -> Self {
        Self {
            account,
            address,
            appointments: vec![],
            selected: 0,
            details: false,
            time_zone,
            today,
            view,
            day: today,
            on_day: 0,
        }
    }

    pub fn view(&self) -> CalendarView {
        self.view
    }

    /// The selected day in the week and the month.
    pub fn day(&self) -> Date {
        self.day
    }

    /// The selected one of the selected day's appointments.
    pub fn selected_on_day(&self) -> usize {
        self.on_day
    }

    /// Shows `appointments`, soonest first. The selection stays on the same
    /// appointment if it is still there. Or else, in the agenda it goes to
    /// today's first, and in the week and the month it stays on its day.
    pub fn set_appointments(&mut self, appointments: Vec<Appointment>) {
        let listed = self.appointments.get(self.selected).map(|a| a.id.clone());
        let on_day = self.selected_of_day().map(|a| a.id.clone());
        self.appointments = appointments;
        self.selected =
            match listed.and_then(|id| self.appointments.iter().position(|a| a.id == id)) {
                Some(index) => index,
                None => self.first_from_today(),
            };
        let day = self.appointments_on(self.day);
        self.on_day = on_day
            .and_then(|id| day.iter().position(|a| a.id == id))
            .unwrap_or_else(|| self.on_day.min(day.len().saturating_sub(1)));
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

    /// The appointment that changing, deleting or showing in full acts on.
    pub fn selected(&self) -> Option<&Appointment> {
        match self.view {
            CalendarView::Agenda => self.appointments.get(self.selected),
            CalendarView::Week | CalendarView::Month => self.selected_of_day(),
        }
    }

    fn selected_of_day(&self) -> Option<&Appointment> {
        self.appointments_on(self.day).get(self.on_day).copied()
    }

    /// The appointments on `day`, including any lasting from an earlier day.
    /// One ending at the very start of `day` is not on it, but one that starts
    /// and ends then is.
    pub fn appointments_on(&self, day: Date) -> Vec<&Appointment> {
        let Some((start, end)) = self.span_of(day, day) else {
            return vec![];
        };
        self.appointments
            .iter()
            .filter(|a| a.start < end && (a.end > start || a.start == start))
            .collect()
    }

    /// The day a new appointment starts on: the selected one, or today
    /// rather than a day gone by.
    pub fn day_for_new(&self) -> Date {
        let day = match self.view {
            CalendarView::Agenda => self.selected().map(|a| self.day_of(a)),
            CalendarView::Week | CalendarView::Month => Some(self.day),
        };
        day.filter(|day| *day >= self.today).unwrap_or(self.today)
    }

    /// Lays the calendar out as `view`, keeping to the same place.
    fn set_view(&mut self, view: CalendarView) {
        match (self.view, view) {
            (CalendarView::Agenda, CalendarView::Week | CalendarView::Month) => {
                if let Some(selected) = self.selected().cloned() {
                    self.day = self.day_of(&selected);
                    self.on_day = self
                        .appointments_on(self.day)
                        .iter()
                        .position(|a| a.id == selected.id)
                        .unwrap_or(0);
                }
            }
            (CalendarView::Week | CalendarView::Month, CalendarView::Agenda) => {
                let chosen = self.selected().map(|a| a.id.clone());
                self.selected = chosen
                    .and_then(|id| self.appointments.iter().position(|a| a.id == id))
                    .or_else(|| {
                        self.appointments
                            .iter()
                            .position(|a| self.day_of(a) >= self.day)
                    })
                    .unwrap_or(self.appointments.len().saturating_sub(1));
            }
            _ => {}
        }
        self.view = view;
    }

    /// The first and the last day laid out, unless in the agenda.
    pub fn visible_days(&self) -> Option<(Date, Date)> {
        match self.view {
            CalendarView::Agenda => None,
            CalendarView::Week => {
                let monday = monday_of(self.day);
                Some((monday, add_days(monday, 6)))
            }
            CalendarView::Month => Some((
                monday_of(self.day.first_of_month()),
                add_days(monday_of(self.day.last_of_month()), 6),
            )),
        }
    }

    /// Every day laid out, in order.
    pub fn visible_dates(&self) -> Vec<Date> {
        let Some((first, last)) = self.visible_days() else {
            return vec![];
        };
        first.series(1.day()).take_while(|d| *d <= last).collect()
    }

    /// From the start of `first` to the end of `last`, if those are times
    /// that exist.
    fn span_of(&self, first: Date, last: Date) -> Option<(Timestamp, Timestamp)> {
        let start = first.to_zoned(self.time_zone.clone()).ok()?;
        let end = last
            .tomorrow()
            .ok()?
            .to_zoned(self.time_zone.clone())
            .ok()?;
        Some((start.timestamp(), end.timestamp()))
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

    /// The line of `rows` with the selected appointment.
    pub fn selected_row(&self, rows: &[Row]) -> usize {
        rows.iter()
            .position(|row| *row == Row::Appointment(self.selected))
            .unwrap_or(0)
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

    /// Acts on a key: moves the selection, or leaves it at either end, or
    /// says what else the key asks for.
    pub fn key(&mut self, key: KeyEvent) -> Request {
        let selected = || self.selected().cloned();
        match key.code {
            KeyCode::Esc if self.details => self.details = false,
            KeyCode::Esc | KeyCode::Char('q') => return Request::Close,
            KeyCode::Enter => self.details = !self.details,
            KeyCode::Char('c') => return Request::New,
            KeyCode::Char('e') => return selected().map_or(Request::None, Request::Edit),
            KeyCode::Char('d') => return selected().map_or(Request::None, Request::Delete),
            KeyCode::F(5) => return Request::Refresh,
            KeyCode::Char('?') => return Request::Help,
            KeyCode::Tab => return Request::NextAccount,
            KeyCode::BackTab => return Request::PrevAccount,
            KeyCode::Char('a') => self.set_view(CalendarView::Agenda),
            KeyCode::Char('w') => self.set_view(CalendarView::Week),
            KeyCode::Char('m') => self.set_view(CalendarView::Month),
            _ if self.view == CalendarView::Agenda => self.move_selection(key),
            _ => self.move_in_grid(key),
        }
        Request::None
    }

    /// Moves about the week or the month, leaving the selection as it is at
    /// the ends of the calendar's dates.
    fn move_in_grid(&mut self, key: KeyEvent) {
        let month = self.view == CalendarView::Month;
        let last_on_day = self.appointments_on(self.day).len().saturating_sub(1);
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => self.step_days(-1),
            KeyCode::Char('l') | KeyCode::Right => self.step_days(1),
            KeyCode::Char('j') | KeyCode::Down if month => self.step_days(7),
            KeyCode::Char('k') | KeyCode::Up if month => self.step_days(-7),
            KeyCode::Char('j' | 'n') | KeyCode::Down => {
                self.on_day = (self.on_day + 1).min(last_on_day);
            }
            KeyCode::Char('k' | 'p') | KeyCode::Up => {
                self.on_day = self.on_day.saturating_sub(1);
            }
            KeyCode::PageDown if month => self.step_months(1),
            KeyCode::PageUp if month => self.step_months(-1),
            KeyCode::PageDown => self.step_days(7),
            KeyCode::PageUp => self.step_days(-7),
            KeyCode::Char('g') | KeyCode::Home => self.go_to_end(false),
            KeyCode::Char('G') | KeyCode::End => self.go_to_end(true),
            KeyCode::Char('t') => self.select_day(self.today),
            _ => {}
        }
    }

    /// Selects `day` and its first appointment.
    fn select_day(&mut self, day: Date) {
        self.day = day;
        self.on_day = 0;
    }

    fn step_days(&mut self, days: i64) {
        if let Ok(day) = self.day.checked_add(days.days()) {
            self.select_day(day);
        }
    }

    /// To the same day of another month, or the last if it has too few.
    fn step_months(&mut self, months: i64) {
        if let Ok(day) = self.day.checked_add(months.months()) {
            self.select_day(day);
        }
    }

    /// To the first or last day of the week or month.
    fn go_to_end(&mut self, last: bool) {
        let day = match (self.view, last) {
            (CalendarView::Month, false) => self.day.first_of_month(),
            (CalendarView::Month, true) => self.day.last_of_month(),
            (_, false) => monday_of(self.day),
            (_, true) => add_days(monday_of(self.day), 6),
        };
        self.select_day(day);
    }

    /// Moves the selection, or leaves it at either end.
    fn move_selection(&mut self, key: KeyEvent) {
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

/// The Monday of `day`'s week, or `day` at the very start of the dates.
fn monday_of(day: Date) -> Date {
    let back = i64::from(day.weekday().to_monday_zero_offset());
    day.checked_sub(back.days()).unwrap_or(day)
}

/// `days` after `day`, or `day` at the very end of the dates.
fn add_days(day: Date, days: i64) -> Date {
    day.checked_add(days.days()).unwrap_or(day)
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
            CalendarView::Agenda,
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

    fn press(agenda: &mut Agenda, code: KeyCode) -> Request {
        agenda.key(KeyEvent::from(code))
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

    #[test]
    fn enter_shows_the_appointment_in_full_and_esc_hides_it_again() {
        let mut agenda = agenda();

        press(&mut agenda, KeyCode::Enter);
        assert!(agenda.details);
        assert_eq!(press(&mut agenda, KeyCode::Esc), Request::None);
        assert!(!agenda.details);
        assert_eq!(press(&mut agenda, KeyCode::Esc), Request::Close);
    }

    #[test]
    fn tab_and_shift_tab_ask_for_the_next_and_previous_accounts_calendar() {
        let mut agenda = agenda();

        assert_eq!(press(&mut agenda, KeyCode::Tab), Request::NextAccount);
        assert_eq!(press(&mut agenda, KeyCode::BackTab), Request::PrevAccount);
    }

    #[test]
    fn keys_ask_to_write_change_delete_or_refresh_appointments() {
        let mut agenda = agenda();
        let today = agenda.selected().unwrap().clone();

        assert_eq!(press(&mut agenda, KeyCode::Char('c')), Request::New);
        assert_eq!(
            press(&mut agenda, KeyCode::Char('e')),
            Request::Edit(today.clone())
        );
        assert_eq!(
            press(&mut agenda, KeyCode::Char('d')),
            Request::Delete(today)
        );
        assert_eq!(press(&mut agenda, KeyCode::F(5)), Request::Refresh);
        assert_eq!(press(&mut agenda, KeyCode::Char('?')), Request::Help);
        assert_eq!(press(&mut agenda, KeyCode::Char('q')), Request::Close);
    }

    /// The same agenda, laid out by the `key` of a view.
    fn in_view(key: char) -> Agenda {
        let mut agenda = agenda();
        press(&mut agenda, KeyCode::Char(key));
        agenda
    }

    fn ids(appointments: &[&Appointment]) -> Vec<String> {
        appointments.iter().map(|a| a.id.0.clone()).collect()
    }

    fn lasting(id: &str, start: &str, end: &str) -> Appointment {
        Appointment {
            end: end.parse().unwrap(),
            ..appointment(id, start)
        }
    }

    #[test]
    fn a_week_starts_on_a_monday_and_runs_over_the_end_of_a_year() {
        let mut agenda = agenda();
        press(&mut agenda, KeyCode::Char('w'));
        assert_eq!(
            agenda.visible_days(),
            Some((date(2026, 11, 2), date(2026, 11, 8)))
        );

        for _ in 0..8 {
            press(&mut agenda, KeyCode::PageDown);
        }

        assert_eq!(agenda.day(), date(2026, 12, 28));
        assert_eq!(
            agenda.visible_days(),
            Some((date(2026, 12, 28), date(2027, 1, 3)))
        );
    }

    #[test]
    fn a_sunday_is_the_last_day_of_its_week() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('h'));

        assert_eq!(agenda.day(), date(2026, 11, 1));
        assert_eq!(
            agenda.visible_days(),
            Some((date(2026, 10, 26), date(2026, 11, 1)))
        );
    }

    #[test]
    fn a_month_lays_out_the_whole_weeks_it_touches() {
        let mut agenda = in_view('m');
        assert_eq!(
            agenda.visible_days(),
            Some((date(2026, 10, 26), date(2026, 12, 6)))
        );

        // February 2027 starts on a Monday and ends on a Sunday: four weeks.
        for _ in 0..3 {
            press(&mut agenda, KeyCode::PageDown);
        }

        assert_eq!(
            agenda.visible_days(),
            Some((date(2027, 2, 1), date(2027, 2, 28)))
        );
    }

    #[test]
    fn the_next_month_from_the_31st_is_its_last_day() {
        let mut agenda = in_view('m');
        // To the last day of November, the first of December, the 31st.
        press(&mut agenda, KeyCode::Char('G'));
        press(&mut agenda, KeyCode::Char('l'));
        press(&mut agenda, KeyCode::Char('G'));
        assert_eq!(agenda.day(), date(2026, 12, 31));

        press(&mut agenda, KeyCode::PageDown);
        press(&mut agenda, KeyCode::PageDown);

        assert_eq!(agenda.day(), date(2027, 2, 28));
    }

    #[test]
    fn up_and_down_move_by_the_week_in_the_month() {
        let mut agenda = in_view('m');

        press(&mut agenda, KeyCode::Char('j'));
        assert_eq!(agenda.day(), date(2026, 11, 9));
        press(&mut agenda, KeyCode::Up);
        press(&mut agenda, KeyCode::Up);
        assert_eq!(agenda.day(), date(2026, 10, 26));
    }

    #[test]
    fn up_and_down_move_between_the_days_appointments_in_the_week() {
        let mut agenda = in_view('w');
        assert_eq!(selected(&agenda), "today 9");

        press(&mut agenda, KeyCode::Char('j'));
        assert_eq!(selected(&agenda), "today 14");
        press(&mut agenda, KeyCode::Down);
        assert_eq!(selected(&agenda), "today 14");
        press(&mut agenda, KeyCode::Char('k'));
        assert_eq!(selected(&agenda), "today 9");
    }

    #[test]
    fn n_and_p_move_between_the_days_appointments_in_the_month_too() {
        let mut agenda = in_view('m');

        press(&mut agenda, KeyCode::Char('n'));
        assert_eq!(selected(&agenda), "today 14");
        press(&mut agenda, KeyCode::Char('p'));
        assert_eq!(selected(&agenda), "today 9");
    }

    #[test]
    fn another_day_selects_its_first_appointment_and_t_goes_back_to_today() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('n'));

        press(&mut agenda, KeyCode::Right);
        assert_eq!(selected(&agenda), "tomorrow");
        press(&mut agenda, KeyCode::Char('t'));
        assert_eq!(agenda.day(), date(2026, 11, 2));
        assert_eq!(selected(&agenda), "today 9");
    }

    #[test]
    fn g_and_shift_g_go_to_the_ends_of_the_week_or_month() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('G'));
        assert_eq!(agenda.day(), date(2026, 11, 8));
        press(&mut agenda, KeyCode::Char('g'));
        assert_eq!(agenda.day(), date(2026, 11, 2));

        let mut agenda = in_view('m');
        press(&mut agenda, KeyCode::End);
        assert_eq!(agenda.day(), date(2026, 11, 30));
        press(&mut agenda, KeyCode::Home);
        assert_eq!(agenda.day(), date(2026, 11, 1));
    }

    #[test]
    fn an_appointment_is_on_every_day_it_touches() {
        let mut agenda = agenda();
        agenda.set_appointments(vec![lasting(
            "trip",
            "2026-11-02T22:00:00Z",
            "2026-11-04T09:00:00Z",
        )]);

        for day in [2, 3, 4] {
            assert_eq!(
                ids(&agenda.appointments_on(date(2026, 11, day))),
                ["trip"],
                "on the {day}th"
            );
        }
        assert!(agenda.appointments_on(date(2026, 11, 5)).is_empty());
    }

    #[test]
    fn a_whole_day_ending_at_midnight_is_not_on_the_next_day() {
        let mut agenda = agenda();
        agenda.set_appointments(vec![Appointment {
            all_day: true,
            ..lasting("holiday", "2026-11-02T00:00:00Z", "2026-11-03T00:00:00Z")
        }]);

        assert_eq!(ids(&agenda.appointments_on(date(2026, 11, 2))), ["holiday"]);
        assert!(agenda.appointments_on(date(2026, 11, 3)).is_empty());
    }

    #[test]
    fn days_follow_the_time_zone_not_utc() {
        let zone = TimeZone::fixed(jiff::tz::offset(2));
        let mut agenda = Agenda::new(
            0,
            String::new(),
            zone,
            date(2026, 11, 2),
            CalendarView::Week,
        );
        agenda.set_appointments(vec![appointment("late", "2026-11-02T23:00:00Z")]);

        assert!(agenda.appointments_on(date(2026, 11, 2)).is_empty());
        assert_eq!(ids(&agenda.appointments_on(date(2026, 11, 3))), ["late"]);
    }

    #[test]
    fn an_appointment_of_no_length_at_midnight_is_on_that_day() {
        let mut agenda = agenda();
        agenda.set_appointments(vec![lasting(
            "reminder",
            "2026-11-03T00:00:00Z",
            "2026-11-03T00:00:00Z",
        )]);

        assert!(agenda.appointments_on(date(2026, 11, 2)).is_empty());
        assert_eq!(
            ids(&agenda.appointments_on(date(2026, 11, 3))),
            ["reminder"]
        );
    }

    #[test]
    fn an_empty_day_has_nothing_to_change_or_delete_but_can_have_a_new_one() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('G'));

        assert!(agenda.selected().is_none());
        assert_eq!(press(&mut agenda, KeyCode::Char('e')), Request::None);
        assert_eq!(press(&mut agenda, KeyCode::Char('d')), Request::None);
        assert_eq!(press(&mut agenda, KeyCode::Char('c')), Request::New);
        assert_eq!(agenda.day_for_new(), date(2026, 11, 8));
    }

    #[test]
    fn a_new_appointment_goes_on_today_rather_than_a_day_gone_by() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('h'));

        assert_eq!(agenda.day(), date(2026, 11, 1));
        assert_eq!(agenda.day_for_new(), date(2026, 11, 2));
    }

    #[test]
    fn changing_the_view_keeps_the_selected_appointment() {
        let mut agenda = agenda();
        press(&mut agenda, KeyCode::Char('j'));
        assert_eq!(selected(&agenda), "today 14");

        press(&mut agenda, KeyCode::Char('w'));
        assert_eq!(agenda.view(), CalendarView::Week);
        assert_eq!(agenda.day(), date(2026, 11, 2));
        assert_eq!(selected(&agenda), "today 14");
        press(&mut agenda, KeyCode::Char('m'));
        assert_eq!(selected(&agenda), "today 14");
        press(&mut agenda, KeyCode::Char('l'));
        press(&mut agenda, KeyCode::Char('a'));

        assert_eq!(agenda.view(), CalendarView::Agenda);
        assert_eq!(selected(&agenda), "tomorrow");
    }

    #[test]
    fn back_in_the_agenda_from_an_empty_day_the_next_appointment_is_selected() {
        let mut agenda = in_view('w');
        // Back to Saturday 31 October, with nothing on it.
        press(&mut agenda, KeyCode::Char('h'));
        press(&mut agenda, KeyCode::Char('h'));
        assert!(agenda.selected().is_none());

        press(&mut agenda, KeyCode::Char('a'));

        assert_eq!(selected(&agenda), "yesterday");
    }

    #[test]
    fn a_refreshed_week_stays_on_the_selected_appointment() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('n'));

        let mut appointments = agenda.appointments().to_vec();
        appointments.insert(1, appointment("earlier", "2026-11-02T08:00:00Z"));
        agenda.set_appointments(appointments);

        assert_eq!(selected(&agenda), "today 14");
    }

    #[test]
    fn with_its_appointment_gone_a_refreshed_week_stays_on_the_day() {
        let mut agenda = in_view('w');
        press(&mut agenda, KeyCode::Char('n'));

        agenda.set_appointments(vec![appointment("today 9", "2026-11-02T09:00:00Z")]);

        assert_eq!(agenda.day(), date(2026, 11, 2));
        assert_eq!(selected(&agenda), "today 9");
    }

    #[test]
    fn an_empty_agenda_has_nothing_to_change_or_delete() {
        let mut agenda = agenda();
        agenda.set_appointments(vec![]);

        assert_eq!(press(&mut agenda, KeyCode::Char('e')), Request::None);
        assert_eq!(press(&mut agenda, KeyCode::Char('d')), Request::None);
        assert_eq!(press(&mut agenda, KeyCode::Char('c')), Request::New);
    }
}
