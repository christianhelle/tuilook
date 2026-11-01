//! What fills the screen, and so takes the keys, under any dialog.

use crate::tui::{
    agenda::Agenda,
    appointment_form::{AppointmentForm, Field as AppointmentField},
    compose::{Compose, Field},
    signature::SignatureEditor,
    vim::VimMode,
};

/// The cursor's shape: a bar while typing, a block in vim's other modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Bar,
    Block,
}

/// A bar in insert mode, a block in vim's others.
fn vim_cursor(mode: VimMode) -> Cursor {
    if mode == VimMode::Insert {
        Cursor::Bar
    } else {
        Cursor::Block
    }
}

/// One screen at a time: which one is the whole of what the app needs to know
/// to route a key or draw a frame.
#[derive(Debug, Default)]
pub enum Screen {
    /// The folders, messages and reading pane.
    #[default]
    Panes,
    /// A message being written.
    Compose(Box<Compose>),
    /// An account's signature being written.
    Signature(Box<SignatureEditor>),
    /// An account's calendar, and the appointment being written or changed
    /// over it, if one is.
    Agenda {
        agenda: Box<Agenda>,
        form: Option<Box<AppointmentForm>>,
    },
}

impl Screen {
    /// The cursor this screen wants.
    pub fn cursor(&self) -> Cursor {
        match self {
            Screen::Panes | Screen::Agenda { form: None, .. } => Cursor::Default,
            Screen::Compose(compose) if compose.field != Field::Body => Cursor::Bar,
            Screen::Compose(compose) => vim_cursor(compose.body.vim.mode),
            Screen::Signature(editor) => vim_cursor(editor.body.vim.mode),
            Screen::Agenda {
                form: Some(form), ..
            } if form.field != AppointmentField::Notes => Cursor::Bar,
            Screen::Agenda {
                form: Some(form), ..
            } => vim_cursor(form.notes.vim.mode),
        }
    }

    /// The message being written, if one is.
    pub fn compose(&self) -> Option<&Compose> {
        match self {
            Screen::Compose(compose) => Some(compose.as_ref()),
            _ => None,
        }
    }

    pub fn compose_mut(&mut self) -> Option<&mut Compose> {
        match self {
            Screen::Compose(compose) => Some(compose.as_mut()),
            _ => None,
        }
    }

    /// The open calendar, if one is.
    pub fn agenda(&self) -> Option<&Agenda> {
        match self {
            Screen::Agenda { agenda, .. } => Some(agenda.as_ref()),
            _ => None,
        }
    }

    pub fn agenda_mut(&mut self) -> Option<&mut Agenda> {
        match self {
            Screen::Agenda { agenda, .. } => Some(agenda.as_mut()),
            _ => None,
        }
    }

    /// Back to the panes, if a calendar is open.
    pub fn close_agenda(&mut self) {
        if self.agenda().is_some() {
            *self = Screen::Panes;
        }
    }

    /// The appointment being written or changed, if one is.
    pub fn appointment(&self) -> Option<&AppointmentForm> {
        match self {
            Screen::Agenda { form, .. } => form.as_deref(),
            _ => None,
        }
    }

    pub fn appointment_mut(&mut self) -> Option<&mut AppointmentForm> {
        match self {
            Screen::Agenda { form, .. } => form.as_deref_mut(),
            _ => None,
        }
    }

    /// Writes an appointment over the open calendar, if one is.
    pub fn open_appointment(&mut self, appointment: AppointmentForm) {
        if let Screen::Agenda { form, .. } = self {
            *form = Some(Box::new(appointment));
        }
    }

    /// Back to the calendar, if an appointment is being written.
    pub fn close_appointment(&mut self) {
        if let Screen::Agenda { form, .. } = self {
            *form = None;
        }
    }

    /// The signature being written, if one is.
    pub fn signature(&self) -> Option<&SignatureEditor> {
        match self {
            Screen::Signature(editor) => Some(editor.as_ref()),
            _ => None,
        }
    }

    pub fn signature_mut(&mut self) -> Option<&mut SignatureEditor> {
        match self {
            Screen::Signature(editor) => Some(editor.as_mut()),
            _ => None,
        }
    }

    /// Back to the panes, if a signature is being written.
    pub fn close_signature(&mut self) {
        if self.signature().is_some() {
            *self = Screen::Panes;
        }
    }

    /// Back to the panes, if a message is being written. A send that
    /// finishes after the user has moved on leaves the other screens alone.
    pub fn close_compose(&mut self) {
        if self.compose().is_some() {
            *self = Screen::Panes;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    use super::Cursor;
    use crate::model::Action;
    use crate::tui::{app::AppState, keymap::Keymap};

    fn press(app: &mut AppState, code: KeyCode) {
        app.on_key(KeyEvent::from(code), &mut Keymap::default());
    }

    #[test]
    fn the_panes_and_the_agenda_keep_the_default_cursor() {
        let mut app = AppState::default();
        assert_eq!(app.cursor(), Cursor::Default);

        app.update(Action::Calendar);

        assert_eq!(app.cursor(), Cursor::Default);
    }

    #[test]
    fn typing_a_search_shows_a_bar() {
        let mut app = AppState::default();

        app.update(Action::Search);

        assert_eq!(app.cursor(), Cursor::Bar);
    }

    #[test]
    fn the_headers_of_a_message_show_a_bar() {
        let mut app = AppState::default();

        app.update(Action::Compose);

        assert_eq!(app.cursor(), Cursor::Bar);
    }

    #[test]
    fn a_signature_shows_a_bar_while_inserting_and_a_block_otherwise() {
        let mut app = AppState::default();
        app.update(Action::EditSignature);

        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.cursor(), Cursor::Bar);

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.cursor(), Cursor::Block);
    }

    #[test]
    fn a_new_appointments_subject_shows_a_bar() {
        let mut app = AppState::default();
        app.update(Action::Calendar);

        press(&mut app, KeyCode::Char('c'));

        assert_eq!(app.cursor(), Cursor::Bar);
    }

    #[test]
    fn a_dialog_taking_text_shows_a_bar_over_the_panes() {
        let app = AppState {
            dialog: Some(crate::tui::dialog::Dialog::Files(
                crate::tui::files::FilePicker::open(std::env::temp_dir()),
            )),
            ..Default::default()
        };

        assert_eq!(app.cursor(), Cursor::Bar);
    }

    #[test]
    fn a_dialog_not_taking_text_leaves_the_screens_cursor() {
        let mut app = AppState::default();
        app.update(Action::Compose);
        app.dialog = Some(crate::tui::dialog::Dialog::Help);

        assert_eq!(app.cursor(), Cursor::Bar);
    }
}
