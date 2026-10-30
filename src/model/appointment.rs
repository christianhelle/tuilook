use jiff::Timestamp;

/// Graph event id. One occurrence of a recurring series has its own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppointmentId(pub String);

/// Something in the calendar: a meeting, or time kept for oneself. Graph
/// calls it an event, but [`Event`](crate::model::Event) is what core tells
/// the UI happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appointment {
    pub id: AppointmentId,
    pub subject: String,
    pub start: Timestamp,
    pub end: Timestamp,
    /// Lasts whole days, from midnight to midnight where it is shown.
    pub all_day: bool,
    pub location: String,
    /// The organizer's name, or address when there is no name.
    pub organizer: String,
    /// The addresses invited.
    pub attendees: Vec<String>,
    pub preview: String,
    /// Whether the account owns it, and so may change it for everyone.
    pub is_organizer: bool,
}

/// An appointment written in tuilook, ready to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppointmentDraft {
    pub subject: String,
    pub start: Timestamp,
    pub end: Timestamp,
    pub all_day: bool,
    pub location: String,
    /// The addresses to invite.
    pub attendees: Vec<String>,
    /// Plain text, or `None` to leave the notes as they are.
    pub notes: Option<String>,
}

impl AppointmentDraft {
    /// Why the appointment cannot be saved as it is, if it cannot.
    pub fn validate(&self) -> Result<(), String> {
        if self.subject.trim().is_empty() {
            return Err("give it a subject".to_owned());
        }
        if self.end < self.start {
            return Err("it ends before it starts".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(subject: &str, start: &str, end: &str) -> AppointmentDraft {
        AppointmentDraft {
            subject: subject.to_owned(),
            start: start.parse().unwrap(),
            end: end.parse().unwrap(),
            all_day: false,
            location: String::new(),
            attendees: vec![],
            notes: None,
        }
    }

    #[test]
    fn an_appointment_with_a_subject_and_a_later_end_can_be_saved() {
        let ok = draft("Standup", "2026-11-02T09:00:00Z", "2026-11-02T09:15:00Z");

        assert_eq!(ok.validate(), Ok(()));
    }

    #[test]
    fn an_appointment_needs_a_subject() {
        let blank = draft("  ", "2026-11-02T09:00:00Z", "2026-11-02T09:15:00Z");

        assert_eq!(blank.validate(), Err("give it a subject".to_owned()));
    }

    #[test]
    fn an_appointment_cannot_end_before_it_starts() {
        let backwards = draft("Standup", "2026-11-02T09:00:00Z", "2026-11-02T08:00:00Z");

        assert_eq!(
            backwards.validate(),
            Err("it ends before it starts".to_owned())
        );
    }
}
