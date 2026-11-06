//! What the reading pane shows of the selected message: its headers, the
//! body and attachments core loaded, any invite, and where it is scrolled.

use jiff::{SignedDuration, Zoned, tz::TimeZone};

use crate::model::{Attachment, Invite, Message, Rsvp};
use crate::tui::{attachments::human_size, ui::shown, viewer::Viewer};

/// The reading pane's state for the selected message. A new selection
/// starts from [`Reader::default`].
#[derive(Debug, Default)]
pub struct Reader {
    /// The body, once core has loaded it.
    pub body: Option<String>,
    /// The attached files, once core has loaded the body.
    pub attachments: Vec<Attachment>,
    /// What the message invites to, once core has said it does.
    pub invite: Option<Invite>,
    /// Lines scrolled out of view at the top.
    pub scroll: usize,
    /// The cursor, which picks text to copy.
    pub viewer: Viewer,
}

impl Reader {
    /// The lines shown for `m`, before wrapping: its headers, then the body,
    /// or its preview until the body has loaded. Times are in `zone`.
    pub fn text(&self, m: &Message, zone: &TimeZone) -> Vec<String> {
        let mut text = vec![
            format!("From: {} <{}>", shown(&m.from_name), shown(&m.from_addr)),
            format!("Subject: {}", shown(&m.subject)),
            format!(
                "Date: {}",
                m.received_at
                    .to_zoned(zone.clone())
                    .strftime("%a %-d %b %Y, %H:%M")
            ),
        ];
        for (index, a) in self.attachments.iter().enumerate() {
            let label = if index == 0 { "Attached:" } else { "" };
            let size = human_size(a.size);
            text.push(format!("{label:<10}{} ({size})", shown(&a.name)));
        }
        if let Some(invite) = &self.invite {
            text.push(format!("When:     {}", when(invite, zone)));
            if !invite.location.is_empty() {
                text.push(format!("Where:    {}", shown(&invite.location)));
            }
            let answer = invite.answer.map_or("Not answered", answered);
            text.push(format!("Answer:   {answer}"));
        }
        text.push(String::new());
        let body = self.body.as_deref().unwrap_or(&m.preview);
        text.extend(shown(body).lines().map(str::to_owned));
        text
    }
}

/// When an invite is for, in `zone`. An all-day one ends at midnight, so on
/// the morning after its last day.
fn when(invite: &Invite, zone: &TimeZone) -> String {
    let start = invite.start.to_zoned(zone.clone());
    let end = invite.end.to_zoned(zone.clone());
    let day = |at: &Zoned| at.strftime("%a %-d %b %Y").to_string();
    if invite.all_day {
        let last = invite.end.saturating_sub(SignedDuration::from_secs(1));
        let last = last.unwrap_or(invite.end).to_zoned(zone.clone());
        return if last.date() == start.date() {
            format!("{}, all day", day(&start))
        } else {
            format!("{} to {}, all day", day(&start), day(&last))
        };
    }
    let until = if end.date() == start.date() {
        end.strftime("%H:%M").to_string()
    } else {
        end.strftime("%a %H:%M").to_string()
    };
    format!("{}, {}–{until}", day(&start), start.strftime("%H:%M"))
}

/// How an answer to an invite reads once given.
pub fn answered(answer: Rsvp) -> &'static str {
    match answer {
        Rsvp::Accept => "Accepted",
        Rsvp::Tentative => "Tentative",
        Rsvp::Decline => "Declined",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Message {
        Message {
            preview: "Preview".to_owned(),
            ..crate::core::fake::message("m1")
        }
    }

    fn file(name: &str, size: u64) -> Attachment {
        Attachment {
            id: crate::model::AttachmentId(name.to_owned()),
            name: name.to_owned(),
            content_type: String::new(),
            size,
            kind: crate::model::AttachmentKind::File,
        }
    }

    fn body_of(reader: &Reader) -> Vec<String> {
        let text = reader.text(&message(), &TimeZone::UTC);
        let blank = text.iter().position(String::is_empty).unwrap();
        text[blank + 1..].to_vec()
    }

    #[test]
    fn the_preview_stands_in_until_the_body_loads() {
        let mut reader = Reader::default();
        assert_eq!(body_of(&reader), ["Preview"]);

        reader.body = Some("Line one\nLine two".to_owned());

        assert_eq!(body_of(&reader), ["Line one", "Line two"]);
    }

    #[test]
    fn attached_files_are_listed_under_the_headers() {
        let reader = Reader {
            attachments: vec![file("a.pdf", 2048), file("b.txt", 10)],
            ..Reader::default()
        };

        let text = reader.text(&message(), &TimeZone::UTC);

        assert_eq!(text[3], "Attached: a.pdf (2 KB)");
        assert_eq!(text[4], "          b.txt (10 B)");
    }
}
