//! What the keys are called: the help shown with `?`, and the key hints in the
//! status bar. The keys themselves are handled by each screen; the tests here
//! hold the two together.

use crate::tui::{
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
        Screen::Agenda { form: None, .. } => {
            " c new · e change · d delete · Enter details · t today · F5 sync · Esc back · ? help"
        }
        Screen::Panes if picking => " y copy · Esc cancel",
        // Saving is offered only once the selected mail's files are known.
        Screen::Panes if app.focus == Pane::Reader && !app.attachments.is_empty() => {
            " v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit · s save files"
        }
        Screen::Panes if app.focus == Pane::Reader => {
            " v V pick · yy copy · ← → next/prev mail · r reply · ? help · q quit"
        }
        Screen::Panes if !app.attachments.is_empty() => {
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
pub const AGENDA_HELP: [(&str, &str); 11] = [
    ("↑ ↓  j k", "Move between appointments"),
    ("g  G  t", "Jump to the first, the last, or today's first"),
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
    (
        "m  p  F Insert  s",
        "Move, pin or flag mail; save its files",
    ),
    ("a  A  F2", "New subfolder, new top folder, rename folder"),
    (
        "c  r  R  f",
        "Write a message, reply, reply to all, forward",
    ),
    ("F5  Ctrl-R", "Sync every folder now"),
    ("/", "Search every folder (Esc goes back)"),
    ("S  C", "Write this account's signature, show its calendar"),
    ("?  q  Ctrl-C", "Show this help, quit"),
    ("", "Press any key to close"),
];
