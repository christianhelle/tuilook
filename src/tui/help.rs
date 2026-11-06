//! What the keys are called: the help shown with `?`, and the key hints in the
//! status bar. The keys themselves are handled by each screen; the tests here
//! hold the two together.

use crate::tui::{
    agenda::CalendarView,
    app::{AppState, Pane},
    appointment_form::Field as AppointmentField,
    compose::Field,
    screen::Screen,
};

/// A row of the help: the keys, and what they do.
pub type Row = (&'static str, &'static str);

/// The help for `screen`.
pub fn table(screen: &Screen) -> &'static [Row] {
    match screen {
        Screen::Panes => &HELP,
        Screen::Compose(_) => &COMPOSE_HELP,
        Screen::Signature(_) => &SIGNATURE_HELP,
        Screen::Agenda { form: Some(_), .. } => &APPOINTMENT_HELP,
        Screen::Agenda { form: None, .. } => &AGENDA_HELP,
    }
}

/// The keys the status bar offers on the screen shown.
pub(crate) fn key_hints(app: &AppState, picking: bool) -> &'static str {
    match &app.screen {
        Screen::Compose(compose) if compose.field == Field::From => {
            " ← → change sender · Ctrl-S send · Tab next field · Esc close · ? help"
        }
        Screen::Compose(_) => " Ctrl-S send · Tab next field · Esc close · ? help",
        Screen::Signature(_) => " Ctrl-S save · Esc close · ? help",
        Screen::Agenda {
            form: Some(form), ..
        } if form.field == AppointmentField::AllDay => {
            " Space all day or not · Ctrl-S save · Tab next field · Esc close · ? help"
        }
        Screen::Agenda { form: Some(_), .. } => {
            " Ctrl-S save · Tab next field · Esc close · ? help"
        }
        Screen::Agenda {
            agenda, form: None, ..
        } => match agenda.view() {
            CalendarView::Agenda => {
                " a w m view · c new · e change · d delete · Enter details · t today · Esc back · ? help"
            }
            CalendarView::Week => {
                " h l day · j k item · PgUp PgDn week · a m view · c new · e change · d delete · ? help"
            }
            CalendarView::Month => {
                " h l day · j k week · n p item · PgUp PgDn month · a w view · c new · e change · ? help"
            }
        },
        Screen::Panes if picking => " y copy · Esc cancel",
        // First, as a narrow status bar cuts off the end.
        Screen::Panes if app.focus == Pane::Reader && app.reader.invite.is_some() => {
            " i respond · v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit"
        }
        Screen::Panes if app.reader.invite.is_some() => {
            " i respond · ? help · Tab switch pane · Enter open · / search · c write · r reply · d delete · q quit"
        }
        // Saving is offered only once the selected mail's files are known.
        Screen::Panes if app.focus == Pane::Reader && !app.reader.attachments.is_empty() => {
            " v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit · s save files"
        }
        Screen::Panes if app.focus == Pane::Reader => {
            " v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit"
        }
        Screen::Panes if !app.reader.attachments.is_empty() => {
            " ? help · Tab switch pane · Enter open · / search · c write · r reply · d delete · q quit · s save files"
        }
        Screen::Panes => {
            " ? help · Tab switch pane · Enter open · / search · c write · r reply · d delete · q quit"
        }
    }
}

/// What each key does while writing a message, shown with `?` in normal mode.
pub const COMPOSE_HELP: [(&str, &str); 13] = [
    ("Tab / Shift-Tab", "Next field / previous field"),
    ("Enter  ↑ ↓", "Move between From, To, Cc and Subject"),
    ("← →  in From", "Change the account it is sent from"),
    ("a, b; c", "Separate several addresses"),
    ("i a I A o O", "Type in the body, insert mode"),
    ("Esc", "Back to normal mode"),
    ("h j k l w b e 0 $ gg G", "Move in the body"),
    ("dd cw x p u Ctrl-R v V", "Edit vim style"),
    ("Ctrl-A", "Attach a file (Tab to it, d takes it off)"),
    ("Ctrl-S", "Send"),
    (
        "Esc in normal mode",
        "Close, asking first if anything was written",
    ),
    ("Ctrl-C", "Quit"),
    ("", "Press any key to close"),
];

/// What each key does in the agenda, shown with `?`.
pub const AGENDA_HELP: [(&str, &str); 15] = [
    ("a  w  m", "Show the agenda, the week or the month"),
    ("↑ ↓  j k", "Next appointment; in the month, the week"),
    ("← →  h l", "Week and month: the day before or after"),
    ("n p", "Week and month: the day's next or previous"),
    ("PgUp PgDn", "Week and month: the one before or after"),
    ("g Home  G End  t", "Jump to the first, the last, or today"),
    ("Enter", "Show the appointment in full, or hide it"),
    ("c", "Write a new appointment"),
    ("e", "Change the appointment"),
    ("d", "Delete it, and cancel it for anyone invited"),
    ("F5", "Sync the calendar now"),
    (
        "Tab / Shift-Tab",
        "Show the next account's calendar / previous",
    ),
    ("Esc  q", "Back to the mail"),
    ("Ctrl-C", "Quit"),
    ("", "Press any key to close"),
];

/// What each key does while writing an appointment, shown with `?` in the
/// notes' normal mode.
pub const APPOINTMENT_HELP: [(&str, &str); 11] = [
    ("Tab / Shift-Tab", "Next field / previous field"),
    ("Enter  ↑ ↓", "Move between the fields above the notes"),
    ("2026-11-30  14:30", "How to write the date and the times"),
    ("Space  in All day", "Make it last the whole day, or not"),
    ("a, b; c", "Separate several addresses to invite"),
    ("i a I A o O", "Type in the notes, insert mode"),
    ("dd cw x p u Ctrl-R v V", "Edit the notes vim style"),
    ("Ctrl-S", "Save, inviting those added"),
    (
        "Esc in normal mode",
        "Close, asking first if anything changed",
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
pub const HELP: [(&str, &str); 23] = [
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
    ("z  b  L", "Zoom or show all, hide folders, move the reader"),
    (
        "Space, Ctrl-A",
        "Mark a message, mark them all (Esc clears)",
    ),
    ("d", "Move marked, selected or folder to Deleted Items"),
    ("D", "Delete marked, selected or folder for good"),
    (
        "m  p  F Insert  s  i",
        "Move, pin, flag mail; save files; answer invite",
    ),
    ("a  A  F2", "New subfolder, new top folder, rename folder"),
    (
        "c  r  R  f",
        "Write a message, reply, reply to all, forward",
    ),
    ("F5  Ctrl-R", "Sync every folder now"),
    ("/", "Search every folder (Esc goes back)"),
    ("S  C", "Write this account's signature, show its calendar"),
    ("M", "Add, sign in again, move or remove accounts"),
    ("?  U  q  Ctrl-C", "Show this help, update tuilook, quit"),
    ("", "Press any key to close"),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::model::{AppointmentId, Invite};
    use crate::tui::keymap::Keymap;

    /// The keys a help table's first column names.
    fn named(table: &[Row]) -> HashSet<String> {
        table
            .iter()
            .flat_map(|(keys, _)| keys.split([' ', ',']))
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// What the help calls a key, and the key itself.
    fn every_key() -> Vec<(String, KeyEvent)> {
        let mut keys = Vec::new();
        for c in ('!'..='~').filter(|c| c.is_ascii_graphic()) {
            keys.push((c.to_string(), KeyEvent::from(KeyCode::Char(c))));
        }
        for c in 'a'..='z' {
            let ctrl = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
            keys.push((format!("Ctrl-{}", c.to_ascii_uppercase()), ctrl));
        }
        for n in 1..=12 {
            keys.push((format!("F{n}"), KeyEvent::from(KeyCode::F(n))));
        }
        for (name, code) in [
            ("Space", KeyCode::Char(' ')),
            ("Enter", KeyCode::Enter),
            ("Tab", KeyCode::Tab),
            ("Shift-Tab", KeyCode::BackTab),
            ("Esc", KeyCode::Esc),
            ("Home", KeyCode::Home),
            ("End", KeyCode::End),
            ("PgUp", KeyCode::PageUp),
            ("PgDn", KeyCode::PageDown),
            ("Insert", KeyCode::Insert),
            ("↑", KeyCode::Up),
            ("↓", KeyCode::Down),
            ("←", KeyCode::Left),
            ("→", KeyCode::Right),
        ] {
            keys.push((name.to_owned(), KeyEvent::from(code)));
        }
        keys
    }

    #[test]
    fn every_key_the_panes_handle_is_in_their_help() {
        let named = named(&HELP);
        let missing: Vec<String> = every_key()
            .into_iter()
            .filter(|(_, key)| Keymap::default().action_for(*key).is_some())
            .map(|(name, _)| name)
            .filter(|name| !named.contains(name))
            // The help says what Esc does where it says what it undoes.
            .filter(|name| name != "Esc")
            .collect();

        assert_eq!(missing, Vec::<String>::new());
    }

    #[test]
    fn every_key_the_help_bar_offers_on_the_panes_is_handled() {
        let app = AppState::default();
        let hints = key_hints(&app, false);
        let handled: Vec<String> = every_key()
            .into_iter()
            .filter(|(_, key)| Keymap::default().action_for(*key).is_some())
            .map(|(name, _)| name)
            .collect();

        for hint in hints.split('·') {
            let key = hint.split_whitespace().next().unwrap();
            assert!(handled.iter().any(|h| h == key), "{key} does nothing");
        }
    }

    #[test]
    fn the_help_bar_offers_to_answer_an_invite_only_when_one_is_shown() {
        let invite = Invite {
            event: AppointmentId("e1".to_owned()),
            start: jiff::Timestamp::UNIX_EPOCH,
            end: jiff::Timestamp::UNIX_EPOCH,
            all_day: false,
            location: String::new(),
            organizer: String::new(),
            answer: None,
            response_requested: true,
        };
        for focus in [Pane::Messages, Pane::Reader] {
            let mut app = AppState {
                focus,
                ..Default::default()
            };
            assert!(!key_hints(&app, false).contains("i respond"));

            app.reader.invite = Some(invite.clone());
            assert!(key_hints(&app, false).starts_with(" i respond"));
        }
    }

    #[test]
    fn every_key_the_agenda_handles_is_in_its_help() {
        let named = named(&AGENDA_HELP);

        // As Agenda::key takes them.
        for key in [
            "a",
            "w",
            "m",
            "↑",
            "↓",
            "j",
            "k",
            "←",
            "→",
            "h",
            "l",
            "n",
            "p",
            "PgUp",
            "PgDn",
            "g",
            "G",
            "Home",
            "End",
            "t",
            "Enter",
            "c",
            "e",
            "d",
            "F5",
            "Tab",
            "Shift-Tab",
            "Esc",
            "q",
            "Ctrl-C",
        ] {
            assert!(named.contains(key), "{key} is missing");
        }
    }
}
