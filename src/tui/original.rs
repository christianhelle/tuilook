//! The message being replied to or forwarded, shown beside the message
//! being written.

use jiff::tz::TimeZone;

use crate::model::{ComposeKind, Message};
use crate::tui::{ui::shown, viewer::Viewer};

/// The pane holding the original: whether it is shown, and the cursor that
/// scrolls it and picks text to copy.
#[derive(Debug)]
pub struct Original {
    /// Whether the pane is shown beside the body.
    pub shown: bool,
    /// The cursor, which scrolls and picks text to copy.
    pub viewer: Viewer,
}

impl Default for Original {
    fn default() -> Self {
        Original {
            shown: true,
            viewer: Viewer::default(),
        }
    }
}

impl Original {
    /// Hides the pane, or shows it again.
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }

    /// What the pane is called: what the message being written does to the
    /// original, who sent it and when.
    pub fn title(&self, kind: &ComposeKind, original: &Message, zone: &TimeZone) -> String {
        let doing = match kind {
            ComposeKind::New => "Original",
            ComposeKind::Reply(_) => "Replying to",
            ComposeKind::ReplyAll(_) => "Replying to all",
            ComposeKind::Forward(_) => "Forwarding",
        };
        let who = if original.from_name.is_empty() {
            shown(&original.from_addr)
        } else {
            shown(&original.from_name)
        };
        let when = original
            .received_at
            .to_zoned(zone.clone())
            .strftime("%a %-d %b %Y");
        format!(" {doing} — {who}, {when} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::MessageId;

    #[test]
    fn the_original_starts_shown_and_toggles() {
        let mut original = Original::default();
        assert!(original.shown);

        original.toggle();
        assert!(!original.shown);

        original.toggle();
        assert!(original.shown);
    }

    /// Sent by Ada on Tuesday 3 November 2026, in UTC.
    fn original() -> Message {
        Message {
            from_name: "Ada Lovelace".to_owned(),
            from_addr: "ada@example.com".to_owned(),
            received_at: "2026-11-03T09:00:00Z".parse().unwrap(),
            ..crate::core::fake::message("m1")
        }
    }

    fn title(kind: ComposeKind, original: &Message) -> String {
        Original::default().title(&kind, original, &TimeZone::UTC)
    }

    fn answering(kind: fn(MessageId) -> ComposeKind) -> ComposeKind {
        kind(MessageId("m1".to_owned()))
    }

    #[test]
    fn the_title_says_what_the_message_being_written_does_to_the_original() {
        for (kind, said) in [
            (answering(ComposeKind::Reply), "Replying to"),
            (answering(ComposeKind::ReplyAll), "Replying to all"),
            (answering(ComposeKind::Forward), "Forwarding"),
        ] {
            assert_eq!(
                title(kind, &original()),
                format!(" {said} — Ada Lovelace, Tue 3 Nov 2026 ")
            );
        }
    }

    #[test]
    fn the_title_names_the_address_when_the_sender_has_no_name() {
        let unnamed = Message {
            from_name: String::new(),
            ..original()
        };

        assert_eq!(
            title(answering(ComposeKind::Reply), &unnamed),
            " Replying to — ada@example.com, Tue 3 Nov 2026 "
        );
    }

    #[test]
    fn the_title_gives_the_date_in_the_readers_own_zone() {
        let copenhagen = TimeZone::get("Europe/Copenhagen").unwrap();
        let late = Message {
            received_at: "2026-11-03T23:30:00Z".parse().unwrap(),
            ..original()
        };

        assert_eq!(
            Original::default().title(&answering(ComposeKind::Reply), &late, &copenhagen),
            " Replying to — Ada Lovelace, Wed 4 Nov 2026 ",
            "half past midnight there"
        );
    }
}
