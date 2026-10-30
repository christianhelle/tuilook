//! The calendar in the app: the agenda that takes over the screen, and the
//! appointment being written or changed from it.

use crate::model::Appointment;

use crate::tui::{
    agenda::{self, Agenda},
    app::{AppState, Confirm, Effect, Status},
    appointment_form::{self, AppointmentForm},
    dialog::Dialog,
};
use ratatui::crossterm::event::KeyEvent;

/// Shown from F5 in the agenda until the calendar has synced.
const SYNCING: &str = "Syncing the calendar…";

impl AppState {
    /// Opens the agenda of the account being looked at, from the cache,
    /// and asks for it to be synced.
    pub(crate) fn open_calendar(&mut self) -> Option<Effect> {
        self.open_calendar_of(self.current_account())
    }

    /// Opens `account`'s agenda, from the cache, and asks for it to be
    /// synced.
    fn open_calendar_of(&mut self, account: usize) -> Option<Effect> {
        let address = self.accounts.get(account).cloned().unwrap_or_default();
        let now = self.now();
        self.agenda = Some(Agenda::new(
            account,
            address,
            now.time_zone().clone(),
            now.date(),
        ));
        Some(Effect::LoadAgenda {
            account,
            sync: true,
        })
    }

    /// A key for the open agenda.
    pub(crate) fn agenda_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let agenda = self.agenda.as_mut()?;
        let account = agenda.account;
        match agenda.key(key) {
            agenda::Request::None => {}
            agenda::Request::Close => self.agenda = None,
            agenda::Request::Help => self.dialog = Some(Dialog::Help),
            agenda::Request::NextAccount => return self.switch_calendar(account, 1),
            agenda::Request::PrevAccount => {
                let back = self.accounts.len().saturating_sub(1);
                return self.switch_calendar(account, back);
            }
            agenda::Request::Refresh => {
                self.show_busy(SYNCING);
                return Some(Effect::SyncCalendar(account));
            }
            agenda::Request::New => {
                let today = agenda.today();
                let day = agenda
                    .selected()
                    .map(|a| agenda.day_of(a))
                    .filter(|day| *day >= today)
                    .unwrap_or(today);
                let form = AppointmentForm::new(day, agenda.time_zone().clone());
                self.appointment = Some(form);
            }
            agenda::Request::Edit(appointment) => {
                let form = AppointmentForm::editing(&appointment, agenda.time_zone().clone());
                self.appointment = Some(form);
            }
            agenda::Request::Delete(appointment) => {
                self.dialog = Some(Dialog::Confirm(Confirm::DeleteAppointment {
                    account,
                    id: appointment.id,
                    subject: appointment.subject,
                }));
            }
        }
        None
    }

    /// Shows the calendar of the account `step` further round from
    /// `account`, unless there is no other.
    fn switch_calendar(&mut self, account: usize, step: usize) -> Option<Effect> {
        let accounts = self.accounts.len();
        if accounts < 2 {
            return None;
        }
        self.open_calendar_of((account + step) % accounts)
    }

    /// A key for the appointment being written or changed.
    pub(crate) fn appointment_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let form = self.appointment.as_mut()?;
        let account = self.agenda.as_ref().map_or(0, |a| a.account);
        match form.key(key, self.viewport.compose, &mut self.register) {
            appointment_form::Request::None => {}
            appointment_form::Request::Save { id, draft } => {
                self.show_busy("Saving…");
                return Some(Effect::SaveAppointment { account, id, draft });
            }
            appointment_form::Request::Close => self.appointment = None,
            appointment_form::Request::ConfirmDiscard => {
                self.dialog = Some(Dialog::Confirm(Confirm::DiscardAppointment));
            }
            appointment_form::Request::Help => self.dialog = Some(Dialog::Help),
            appointment_form::Request::Invalid(why) => self.show_error(why),
        }
        None
    }

    /// Closes the appointment core saved, back to the agenda.
    pub fn appointment_saved(&mut self) {
        self.appointment = None;
        self.show_info("Saved");
    }

    /// Lets the appointment being saved be saved again: core could not.
    pub fn appointment_not_saved(&mut self) {
        if let Some(form) = &mut self.appointment {
            form.saving = false;
        }
    }

    /// An appointment of `account` was deleted: its agenda shows the
    /// calendar without it.
    pub fn appointment_deleted(&mut self, account: usize) -> Option<Effect> {
        self.show_info("Deleted");
        self.calendar_synced(account)
    }

    /// Shows `appointments`, `account`'s from the cache, if its agenda is
    /// open.
    pub fn appointments_loaded(&mut self, account: usize, appointments: Vec<Appointment>) {
        if let Some(agenda) = self.agenda.as_mut().filter(|a| a.account == account) {
            agenda.set_appointments(appointments);
        }
    }

    /// `account`'s calendar in the cache changed: its open agenda shows it
    /// again.
    pub fn calendar_synced(&mut self, account: usize) -> Option<Effect> {
        // An F5 waiting for this gets an answer, unless an error took its place.
        if self.status == Some(Status::Busy(SYNCING.to_owned())) {
            self.show_info("Up to date");
        }
        self.agenda
            .as_ref()
            .filter(|a| a.account == account)
            .map(|_| Effect::LoadAgenda {
                account,
                sync: false,
            })
    }
}

#[cfg(test)]
mod tests {
    use jiff::{SignedDuration, Timestamp, tz::TimeZone};
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    use crate::model::{Action, AppointmentId};
    use crate::tui::{app::Confirm, appointment_form::Field, keymap::Keymap};

    use super::*;

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn appointment(id: &str, start: &str) -> Appointment {
        Appointment {
            id: AppointmentId(id.to_owned()),
            subject: id.to_owned(),
            start: at(start),
            end: at(start) + SignedDuration::from_hours(1),
            all_day: false,
            location: String::new(),
            organizer: String::new(),
            attendees: vec![],
            preview: String::new(),
            is_organizer: true,
        }
    }

    /// The agenda open on 2 November 2026 in UTC, on tomorrow's appointment.
    fn in_agenda() -> AppState {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned()],
            clock: Some(at("2026-11-02T08:00:00Z").to_zoned(TimeZone::UTC)),
            ..Default::default()
        };
        app.update(Action::Calendar);
        app.appointments_loaded(0, vec![appointment("tomorrow", "2026-11-03T09:00:00Z")]);
        app
    }

    fn press(app: &mut AppState, code: KeyCode) -> Option<Effect> {
        app.on_key(KeyEvent::from(code), &mut Keymap::default())
    }

    fn typing(app: &mut AppState, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    fn ctrl_s(app: &mut AppState) -> Option<Effect> {
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        app.on_key(ctrl_s, &mut Keymap::default())
    }

    #[test]
    fn c_writes_a_new_appointment_on_the_selected_day() {
        let mut app = in_agenda();

        press(&mut app, KeyCode::Char('c'));

        let form = app.appointment.as_ref().unwrap();
        assert_eq!(form.editing, None);
        assert_eq!(form.date.text(), "2026-11-03");
        assert_eq!(form.field, Field::Subject);
    }

    #[test]
    fn saving_the_new_appointment_asks_core_to_save_it_in_the_agendas_account() {
        let mut app = in_agenda();
        press(&mut app, KeyCode::Char('c'));
        typing(&mut app, "Planning");

        let Some(Effect::SaveAppointment { account, id, draft }) = ctrl_s(&mut app) else {
            panic!("saving asks core to save");
        };

        assert_eq!((account, id), (0, None));
        assert_eq!(draft.subject, "Planning");
        assert_eq!(draft.start, at("2026-11-03T09:00:00Z"));
        assert_eq!(app.status, Some(Status::Busy("Saving…".to_owned())));
    }

    #[test]
    fn e_changes_the_selected_appointment() {
        let mut app = in_agenda();

        press(&mut app, KeyCode::Char('e'));

        let form = app.appointment.as_ref().unwrap();
        assert_eq!(form.editing, Some(AppointmentId("tomorrow".to_owned())));
        assert_eq!(form.subject.text(), "tomorrow");
    }

    #[test]
    fn a_saved_appointment_closes_its_form_back_to_the_agenda() {
        let mut app = in_agenda();
        press(&mut app, KeyCode::Char('e'));
        ctrl_s(&mut app);

        app.appointment_saved();

        assert!(app.appointment.is_none());
        assert!(app.agenda.is_some());
        assert_eq!(app.status, Some(Status::Info("Saved".to_owned())));
    }

    #[test]
    fn an_appointment_that_failed_to_save_can_be_saved_again() {
        let mut app = in_agenda();
        press(&mut app, KeyCode::Char('e'));
        ctrl_s(&mut app);

        app.appointment_not_saved();

        assert!(!app.appointment.as_ref().unwrap().saving);
    }

    #[test]
    fn closing_a_changed_appointment_asks_first() {
        let mut app = in_agenda();
        press(&mut app, KeyCode::Char('c'));
        typing(&mut app, "x");

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.confirm(), Some(&Confirm::DiscardAppointment));
        press(&mut app, KeyCode::Char('y'));

        assert!(app.appointment.is_none());
        assert!(app.agenda.is_some());
    }

    #[test]
    fn d_asks_before_deleting_the_selected_appointment() {
        let mut app = in_agenda();

        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(
            app.confirm(),
            Some(Confirm::DeleteAppointment { .. })
        ));

        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            Some(Effect::DeleteAppointment {
                account: 0,
                id: AppointmentId("tomorrow".to_owned()),
            })
        );
    }

    /// The agenda open on the first of three accounts.
    fn in_three_agendas() -> AppState {
        let mut app = in_agenda();
        app.accounts = ["me@outlook.com", "me@work.example", "me@uni.example"]
            .map(str::to_owned)
            .to_vec();
        app.agenda = None;
        app.update(Action::Calendar);
        app
    }

    #[test]
    fn tab_shows_the_next_accounts_calendar_and_syncs_it() {
        let mut app = in_three_agendas();

        assert_eq!(
            press(&mut app, KeyCode::Tab),
            Some(Effect::LoadAgenda {
                account: 1,
                sync: true,
            })
        );

        let agenda = app.agenda.as_ref().unwrap();
        assert_eq!(
            (agenda.account, agenda.address.as_str()),
            (1, "me@work.example")
        );
    }

    #[test]
    fn shift_tab_from_the_first_account_goes_round_to_the_last() {
        let mut app = in_three_agendas();

        press(&mut app, KeyCode::BackTab);

        assert_eq!(app.agenda.as_ref().unwrap().account, 2);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.agenda.as_ref().unwrap().account, 0);
    }

    #[test]
    fn with_one_account_tab_stays_on_its_calendar() {
        let mut app = in_agenda();

        assert_eq!(press(&mut app, KeyCode::Tab), None);

        let agenda = app.agenda.as_ref().unwrap();
        assert_eq!(agenda.account, 0);
        assert_eq!(agenda.appointments().len(), 1);
    }

    #[test]
    fn f5_says_the_calendar_is_syncing_until_it_is_up_to_date() {
        let mut app = in_agenda();

        assert_eq!(
            press(&mut app, KeyCode::F(5)),
            Some(Effect::SyncCalendar(0))
        );
        assert_eq!(
            app.status,
            Some(Status::Busy("Syncing the calendar…".to_owned()))
        );
        app.calendar_synced(0);

        assert_eq!(app.status, Some(Status::Info("Up to date".to_owned())));
    }

    #[test]
    fn a_deleted_appointment_shows_the_agenda_again() {
        let mut app = in_agenda();

        assert_eq!(
            app.appointment_deleted(0),
            Some(Effect::LoadAgenda {
                account: 0,
                sync: false,
            })
        );
        assert_eq!(app.status, Some(Status::Info("Deleted".to_owned())));
    }
}
