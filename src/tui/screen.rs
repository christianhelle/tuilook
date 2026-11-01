//! What fills the screen, and so takes the keys, under any dialog.

use crate::tui::{
    agenda::Agenda, appointment_form::AppointmentForm, compose::Compose, signature::SignatureEditor,
};

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
