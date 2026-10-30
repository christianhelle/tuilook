//! The calendar in the app: the agenda that takes over the screen, and the
//! appointment being written or changed from it.

use crate::model::Appointment;

use crate::tui::{
    agenda::{self, Agenda},
    app::{AppState, Effect},
    dialog::Dialog,
};
use ratatui::crossterm::event::KeyEvent;

impl AppState {
    /// Opens the agenda of the account being looked at, from the cache,
    /// and asks for it to be synced.
    pub(crate) fn open_calendar(&mut self) -> Option<Effect> {
        let account = self.current_account();
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
            agenda::Request::Refresh => {
                self.show_busy("Syncing the calendar…");
                return Some(Effect::SyncCalendar(account));
            }
            // Writing and changing appointments comes next.
            agenda::Request::New | agenda::Request::Edit(_) | agenda::Request::Delete(_) => {}
        }
        None
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
        self.agenda
            .as_ref()
            .filter(|a| a.account == account)
            .map(|_| Effect::LoadAgenda {
                account,
                sync: false,
            })
    }
}
