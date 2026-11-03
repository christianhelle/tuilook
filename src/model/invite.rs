use jiff::Timestamp;

use crate::model::AppointmentId;

/// An answer to an invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rsvp {
    Accept,
    Tentative,
    Decline,
}

/// The meeting a message invites the account to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// The meeting in the account's calendar, which is what gets answered.
    pub event: AppointmentId,
    pub start: Timestamp,
    pub end: Timestamp,
    /// Lasts whole days, from midnight to midnight where it is shown.
    pub all_day: bool,
    pub location: String,
    /// The organizer's name, or address when there is no name.
    pub organizer: String,
    /// How the account has answered, wherever it did, if it has.
    pub answer: Option<Rsvp>,
    /// Whether the organizer wants to be told the answer.
    pub response_requested: bool,
}
