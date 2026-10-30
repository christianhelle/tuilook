//! Writing an appointment, or changing one: its fields, and notes edited
//! vim style.

use jiff::{
    ToSpan,
    civil::{Date, Time},
    tz::TimeZone,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{Appointment, AppointmentDraft, AppointmentId, parse_addresses};
use crate::tui::{
    editor::Editor,
    input::TextInput,
    vim::{self, Outcome, Register},
};

/// The part of the appointment keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Subject,
    Date,
    Start,
    End,
    AllDay,
    Location,
    Attendees,
    Notes,
}

impl Field {
    const ORDER: [Field; 8] = [
        Field::Subject,
        Field::Date,
        Field::Start,
        Field::End,
        Field::AllDay,
        Field::Location,
        Field::Attendees,
        Field::Notes,
    ];

    fn index(self) -> usize {
        Self::ORDER.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// What a key asks of the app, beyond editing the appointment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    /// Save the appointment: a new one, or a change to the one with this id.
    Save {
        id: Option<AppointmentId>,
        draft: AppointmentDraft,
    },
    /// Close the appointment; nothing in it has changed.
    Close,
    /// Close the appointment once the user agrees to lose the changes.
    ConfirmDiscard,
    Help,
    /// The appointment cannot be saved as it is, for this reason.
    Invalid(String),
}

/// An appointment being written or changed.
#[derive(Debug)]
pub struct AppointmentForm {
    /// The appointment being changed, or `None` for a new one.
    pub editing: Option<AppointmentId>,
    pub subject: TextInput,
    /// `YYYY-MM-DD`.
    pub date: TextInput,
    /// `HH:MM`.
    pub start: TextInput,
    /// `HH:MM`.
    pub end: TextInput,
    pub all_day: bool,
    pub location: TextInput,
    pub attendees: TextInput,
    pub notes: Editor,
    pub field: Field,
    /// Whether the appointment has gone to core to save, so keys wait.
    pub saving: bool,
    /// The days an all-day appointment lasts, kept when it is changed.
    days: i64,
    time_zone: TimeZone,
    /// The fields as they were opened, to tell whether anything changed.
    opened: Vec<String>,
}

impl AppointmentForm {
    /// Starts a new appointment on `day`, from nine to ten in `time_zone`.
    pub fn new(day: Date, time_zone: TimeZone) -> Self {
        let mut form = Self {
            editing: None,
            subject: TextInput::default(),
            date: TextInput::from(day.to_string().as_str()),
            start: TextInput::from("09:00"),
            end: TextInput::from("10:00"),
            all_day: false,
            location: TextInput::default(),
            attendees: TextInput::default(),
            notes: Editor::default(),
            field: Field::Subject,
            saving: false,
            days: 1,
            time_zone,
            opened: vec![],
        };
        form.opened = form.fields();
        form
    }

    /// Starts changing `appointment`, its times shown in `time_zone`.
    pub fn editing(appointment: &Appointment, time_zone: TimeZone) -> Self {
        let start = appointment.start.to_zoned(time_zone.clone());
        let end = appointment.end.to_zoned(time_zone.clone());
        let mut form = Self::new(start.date(), time_zone);
        form.editing = Some(appointment.id.clone());
        form.subject = TextInput::from(appointment.subject.as_str());
        form.start = TextInput::from(start.strftime("%H:%M").to_string().as_str());
        form.end = TextInput::from(end.strftime("%H:%M").to_string().as_str());
        form.all_day = appointment.all_day;
        form.days = (end.date() - start.date()).get_days().max(1).into();
        form.location = TextInput::from(appointment.location.as_str());
        form.attendees = TextInput::from(appointment.attendees.join(", ").as_str());
        form.notes.set_text(&appointment.preview);
        form.notes.move_to(0, 0);
        form.opened = form.fields();
        form
    }

    /// Every field's text, to compare with how they were opened.
    fn fields(&self) -> Vec<String> {
        vec![
            self.subject.text().to_owned(),
            self.date.text().to_owned(),
            self.start.text().to_owned(),
            self.end.text().to_owned(),
            self.all_day.to_string(),
            self.location.text().to_owned(),
            self.attendees.text().to_owned(),
            self.notes.text(),
        ]
    }

    /// Whether `field` is one keys can reach: an all-day appointment has no
    /// times.
    pub fn shows(&self, field: Field) -> bool {
        !(self.all_day && matches!(field, Field::Start | Field::End))
    }

    fn step(&mut self, by: usize) {
        let count = Field::ORDER.len();
        let mut index = self.field.index();
        loop {
            index = (index + by) % count;
            if self.shows(Field::ORDER[index]) {
                break;
            }
        }
        self.field = Field::ORDER[index];
    }

    fn next_field(&mut self) {
        self.step(1);
    }

    fn prev_field(&mut self) {
        self.step(Field::ORDER.len() - 1);
    }

    /// Acts on a key, paging the notes by `page` lines. Text is deleted or
    /// yanked into `register` and put from it.
    pub fn key(&mut self, key: KeyEvent, page: usize, register: &mut Register) -> Request {
        if self.saving {
            return Request::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if ctrl => return self.save(),
            KeyCode::Tab => self.next_field(),
            KeyCode::BackTab => self.prev_field(),
            _ if self.field == Field::Notes => return self.notes_key(key, page, register),
            _ => return self.field_key(key),
        }
        Request::None
    }

    fn notes_key(&mut self, key: KeyEvent, page: usize, register: &mut Register) -> Request {
        let outcome = vim::key(&mut self.notes, key, page, register);
        self.notes.follow(page);
        match outcome {
            Outcome::Leave => self.close(),
            Outcome::Unhandled if key.code == KeyCode::Char('?') => Request::Help,
            Outcome::Handled | Outcome::Yanked(_) | Outcome::Unhandled => Request::None,
        }
    }

    fn field_key(&mut self, key: KeyEvent) -> Request {
        match key.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Enter | KeyCode::Down => self.next_field(),
            KeyCode::Up => self.prev_field(),
            KeyCode::Char(' ') if self.field == Field::AllDay => self.all_day = !self.all_day,
            _ => {
                let input = match self.field {
                    Field::Subject => &mut self.subject,
                    Field::Date => &mut self.date,
                    Field::Start => &mut self.start,
                    Field::End => &mut self.end,
                    Field::Location => &mut self.location,
                    Field::Attendees => &mut self.attendees,
                    Field::AllDay | Field::Notes => return Request::None,
                };
                input.key(key);
            }
        }
        Request::None
    }

    fn close(&self) -> Request {
        if self.fields() == self.opened {
            Request::Close
        } else {
            Request::ConfirmDiscard
        }
    }

    /// The appointment as it now stands, or why it cannot be saved.
    pub fn draft(&self) -> Result<AppointmentDraft, String> {
        let written = self.date.text().trim();
        let date: Date = written
            .parse()
            .map_err(|_| format!("{written} is not a date like 2026-11-30"))?;
        let (start, end) = if self.all_day {
            let end = date
                .checked_add(self.days.days())
                .map_err(|e| e.to_string())?;
            (
                date.to_datetime(Time::midnight()),
                end.to_datetime(Time::midnight()),
            )
        } else {
            (
                date.to_datetime(time(&self.start)?),
                date.to_datetime(time(&self.end)?),
            )
        };
        let instant = |at: jiff::civil::DateTime| {
            at.to_zoned(self.time_zone.clone())
                .map(|zoned| zoned.timestamp())
                .map_err(|e| e.to_string())
        };
        let attendees = parse_addresses(self.attendees.text());
        if let Some(bad) = attendees.iter().find(|a| !a.contains('@')) {
            return Err(format!("{bad} is not an email address"));
        }
        let notes = self.notes.text();
        // A change shows only a preview of the notes, so they are sent only
        // when written anew.
        let unchanged = self.editing.is_some() && self.opened.last() == Some(&notes);
        let draft = AppointmentDraft {
            subject: self.subject.text().trim().to_owned(),
            start: instant(start)?,
            end: instant(end)?,
            all_day: self.all_day,
            location: self.location.text().trim().to_owned(),
            attendees,
            notes: (!unchanged).then_some(notes),
        };
        draft.validate()?;
        Ok(draft)
    }

    fn save(&mut self) -> Request {
        match self.draft() {
            Ok(draft) => {
                self.saving = true;
                Request::Save {
                    id: self.editing.clone(),
                    draft,
                }
            }
            Err(reason) => Request::Invalid(reason),
        }
    }
}

/// The time written in `input`, as `HH:MM`.
fn time(input: &TextInput) -> Result<Time, String> {
    let written = input.text().trim();
    written
        .parse()
        .map_err(|_| format!("{written} is not a time like 14:30"))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn copenhagen() -> TimeZone {
        TimeZone::get("Europe/Copenhagen").unwrap()
    }

    fn at(time: &str) -> jiff::Timestamp {
        time.parse().unwrap()
    }

    fn new_form() -> AppointmentForm {
        AppointmentForm::new(date(2026, 11, 2), copenhagen())
    }

    fn press(form: &mut AppointmentForm, code: KeyCode) -> Request {
        form.key(KeyEvent::from(code), 10, &mut Register::default())
    }

    /// Types `keys`, where ⎋ is Esc, ⇥ is Tab and ⌫ is Backspace.
    fn typing(form: &mut AppointmentForm, keys: &str) -> Request {
        let mut last = Request::None;
        for c in keys.chars() {
            let code = match c {
                '⎋' => KeyCode::Esc,
                '⇥' => KeyCode::Tab,
                '⌫' => KeyCode::Backspace,
                c => KeyCode::Char(c),
            };
            last = press(form, code);
        }
        last
    }

    fn ctrl_s(form: &mut AppointmentForm) -> Request {
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        form.key(ctrl_s, 10, &mut Register::default())
    }

    /// Empties the field keys are in, of up to 20 characters.
    fn clear(form: &mut AppointmentForm) {
        typing(form, &"⌫".repeat(20));
    }

    fn existing() -> Appointment {
        Appointment {
            id: AppointmentId("e1".to_owned()),
            subject: "Review".to_owned(),
            start: at("2026-11-03T13:00:00Z"),
            end: at("2026-11-03T14:30:00Z"),
            all_day: false,
            location: "Room 4".to_owned(),
            organizer: String::new(),
            attendees: vec!["ada@example.com".to_owned(), "grace@example.com".to_owned()],
            preview: "Bring the numbers".to_owned(),
            is_organizer: true,
        }
    }

    #[test]
    fn a_new_appointment_starts_in_the_subject_on_the_day_from_nine_to_ten() {
        let form = new_form();

        assert_eq!(form.field, Field::Subject);
        assert_eq!(form.date.text(), "2026-11-02");
        assert_eq!((form.start.text(), form.end.text()), ("09:00", "10:00"));
    }

    #[test]
    fn saving_a_new_appointment_asks_to_save_it_in_local_time() {
        let mut form = new_form();
        typing(&mut form, "Planning⇥⇥");
        clear(&mut form);
        typing(
            &mut form,
            "09:30⇥⇥⇥Room 4⇥ada@example.com; grace@example.com⇥",
        );
        typing(&mut form, "iBring the numbers");

        assert_eq!(
            ctrl_s(&mut form),
            Request::Save {
                id: None,
                draft: AppointmentDraft {
                    subject: "Planning".to_owned(),
                    // Copenhagen is an hour ahead of UTC in November.
                    start: at("2026-11-02T08:30:00Z"),
                    end: at("2026-11-02T09:00:00Z"),
                    all_day: false,
                    location: "Room 4".to_owned(),
                    attendees: vec!["ada@example.com".to_owned(), "grace@example.com".to_owned()],
                    notes: Some("Bring the numbers".to_owned()),
                },
            }
        );
        assert!(form.saving);
    }

    #[test]
    fn an_all_day_appointment_skips_the_times_and_lasts_the_day() {
        let mut form = new_form();
        typing(&mut form, "Holiday⇥⇥");
        assert_eq!(form.field, Field::Start);
        typing(&mut form, "⇥⇥ ");
        assert!(form.all_day);
        typing(&mut form, "⇥");
        assert_eq!(form.field, Field::Location);
        press(&mut form, KeyCode::BackTab);
        press(&mut form, KeyCode::BackTab);
        assert_eq!(form.field, Field::Date);

        let Request::Save { draft, .. } = ctrl_s(&mut form) else {
            panic!("an all-day appointment saves");
        };
        assert!(draft.all_day);
        assert_eq!(
            (draft.start, draft.end),
            (at("2026-11-01T23:00:00Z"), at("2026-11-02T23:00:00Z"))
        );
    }

    #[test]
    fn a_date_or_time_written_wrong_says_how_to_write_it() {
        let mut form = new_form();
        typing(&mut form, "Planning⇥");
        clear(&mut form);
        typing(&mut form, "2026-13-01");
        assert_eq!(
            ctrl_s(&mut form),
            Request::Invalid("2026-13-01 is not a date like 2026-11-30".to_owned())
        );
        clear(&mut form);
        typing(&mut form, "2026-11-02⇥");
        clear(&mut form);
        typing(&mut form, "9.30");
        assert_eq!(
            ctrl_s(&mut form),
            Request::Invalid("9.30 is not a time like 14:30".to_owned())
        );
        assert!(!form.saving);
    }

    #[test]
    fn attendees_must_be_email_addresses() {
        let mut form = new_form();
        typing(&mut form, "Planning⇥⇥⇥⇥⇥⇥ada");

        assert_eq!(
            ctrl_s(&mut form),
            Request::Invalid("ada is not an email address".to_owned())
        );
    }

    #[test]
    fn an_appointment_needs_a_subject_to_save() {
        let mut form = new_form();

        assert_eq!(
            ctrl_s(&mut form),
            Request::Invalid("give it a subject".to_owned())
        );
    }

    #[test]
    fn changing_an_appointment_starts_from_it_and_saves_under_its_id() {
        let mut form = AppointmentForm::editing(&existing(), copenhagen());
        assert_eq!(form.date.text(), "2026-11-03");
        assert_eq!((form.start.text(), form.end.text()), ("14:00", "15:30"));
        assert_eq!(form.attendees.text(), "ada@example.com, grace@example.com");
        typing(&mut form, " notes");

        assert_eq!(
            ctrl_s(&mut form),
            Request::Save {
                id: Some(AppointmentId("e1".to_owned())),
                draft: AppointmentDraft {
                    subject: "Review notes".to_owned(),
                    start: at("2026-11-03T13:00:00Z"),
                    end: at("2026-11-03T14:30:00Z"),
                    all_day: false,
                    location: "Room 4".to_owned(),
                    attendees: vec!["ada@example.com".to_owned(), "grace@example.com".to_owned()],
                    // Only a preview of the notes was shown, so they stay.
                    notes: None,
                },
            }
        );
    }

    #[test]
    fn a_changed_all_day_appointment_keeps_its_days() {
        let mut long = existing();
        long.all_day = true;
        long.start = at("2026-11-04T23:00:00Z");
        long.end = at("2026-11-07T23:00:00Z");
        let mut form = AppointmentForm::editing(&long, copenhagen());

        let Request::Save { draft, .. } = ctrl_s(&mut form) else {
            panic!("it saves");
        };
        assert_eq!((draft.start, draft.end), (long.start, long.end));
    }

    #[test]
    fn closing_an_untouched_appointment_closes_and_a_changed_one_asks_first() {
        let mut form = new_form();
        assert_eq!(press(&mut form, KeyCode::Esc), Request::Close);

        typing(&mut form, "x");
        assert_eq!(press(&mut form, KeyCode::Esc), Request::ConfirmDiscard);
    }
}
