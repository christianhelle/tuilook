//! The message list's date groups, as Outlook shows them: Today, Yesterday,
//! This Week and so on, each under a header that folds its messages away.

use std::collections::BTreeSet;

use jiff::{Timestamp, ToSpan, Zoned, civil::Date, tz::TimeZone};
use serde::{Deserialize, Serialize};

use crate::model::Message;

/// A span of days the message list groups mail by, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DateGroup {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Older,
}

impl DateGroup {
    pub fn title(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::ThisWeek => "This Week",
            Self::LastWeek => "Last Week",
            Self::ThisMonth => "This Month",
            Self::LastMonth => "Last Month",
            Self::Older => "Older",
        }
    }
}

/// The group of mail received at `received`, seen from `today` in `tz`.
/// Weeks start on Monday. Each group starts no later than the one before
/// it, so the groups follow each other down a newest-first list: when
/// yesterday was in last week, it is still Yesterday.
pub fn group_of(received: Timestamp, today: Date, tz: &TimeZone) -> DateGroup {
    let day = received.to_zoned(tz.clone()).date();
    let back = |days: i64| today.checked_sub(days.days()).unwrap_or(Date::MIN);
    let monday = back(today.weekday().to_monday_zero_offset().into());
    let first = today.first_of_month();
    let starts = [
        (DateGroup::Today, today),
        (DateGroup::Yesterday, back(1)),
        (DateGroup::ThisWeek, monday),
        (
            DateGroup::LastWeek,
            monday.checked_sub(7.days()).unwrap_or(Date::MIN),
        ),
        (DateGroup::ThisMonth, first),
        (
            DateGroup::LastMonth,
            first.checked_sub(1.month()).unwrap_or(Date::MIN),
        ),
    ];
    let mut latest = today;
    for (group, start) in starts {
        latest = latest.min(start);
        if day >= latest {
            return group;
        }
    }
    DateGroup::Older
}

/// A row of the message list: a group's header, or a message by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRow {
    Header {
        group: DateGroup,
        /// Messages in the group, shown or folded away.
        count: usize,
        collapsed: bool,
    },
    Message(usize),
}

/// The rows of newest-first `messages` seen at `now`: each group's header,
/// then its messages unless the group is in `collapsed`.
pub fn message_rows(
    messages: &[Message],
    collapsed: &BTreeSet<DateGroup>,
    now: &Zoned,
) -> Vec<MessageRow> {
    let today = now.date();
    let tz = now.time_zone();
    let mut rows = Vec::with_capacity(messages.len() + 7);
    // The group being filled, and where its header is.
    let mut current: Option<(DateGroup, usize)> = None;
    for (index, message) in messages.iter().enumerate() {
        let group = group_of(message.received_at, today, tz);
        let folded = collapsed.contains(&group);
        let header = match current {
            Some((filling, header)) if filling == group => header,
            _ => {
                rows.push(MessageRow::Header {
                    group,
                    count: 0,
                    collapsed: folded,
                });
                current = Some((group, rows.len() - 1));
                rows.len() - 1
            }
        };
        if let MessageRow::Header { count, .. } = &mut rows[header] {
            *count += 1;
        }
        if !folded {
            rows.push(MessageRow::Message(index));
        }
    }
    rows
}

/// The group the message at `index` is in.
pub fn group_at(messages: &[Message], index: usize, now: &Zoned) -> Option<DateGroup> {
    let message = messages.get(index)?;
    Some(group_of(message.received_at, now.date(), now.time_zone()))
}

#[cfg(test)]
mod tests {
    use crate::model::{FolderId, MessageId};

    use super::*;

    /// Wednesday 28 October 2026, noon UTC.
    fn now() -> Zoned {
        "2026-10-28T12:00:00[UTC]".parse().unwrap()
    }

    fn group(day: &str, today: &str) -> DateGroup {
        let at: Timestamp = format!("{day}T09:00:00Z").parse().unwrap();
        group_of(at, today.parse().unwrap(), &TimeZone::UTC)
    }

    fn message(id: &str, day: &str) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("inbox".to_owned()),
            conversation_id: String::new(),
            subject: String::new(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: format!("{day}T09:00:00Z").parse().unwrap(),
            is_read: false,
            flagged: false,
            pinned: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    #[test]
    fn mail_falls_in_the_groups_outlook_shows() {
        let today = "2026-10-28";
        assert_eq!(group("2026-10-28", today), DateGroup::Today);
        assert_eq!(group("2026-10-27", today), DateGroup::Yesterday);
        assert_eq!(group("2026-10-26", today), DateGroup::ThisWeek, "Monday");
        assert_eq!(group("2026-10-25", today), DateGroup::LastWeek, "Sunday");
        assert_eq!(group("2026-10-19", today), DateGroup::LastWeek);
        assert_eq!(group("2026-10-18", today), DateGroup::ThisMonth);
        assert_eq!(group("2026-10-01", today), DateGroup::ThisMonth);
        assert_eq!(group("2026-09-30", today), DateGroup::LastMonth);
        assert_eq!(group("2026-09-01", today), DateGroup::LastMonth);
        assert_eq!(group("2026-08-31", today), DateGroup::Older);
    }

    #[test]
    fn mail_from_the_future_is_today() {
        assert_eq!(group("2026-11-02", "2026-10-28"), DateGroup::Today);
    }

    #[test]
    fn on_a_monday_yesterday_is_still_yesterday() {
        let monday = "2026-10-26";
        assert_eq!(group("2026-10-25", monday), DateGroup::Yesterday);
        assert_eq!(group("2026-10-24", monday), DateGroup::LastWeek);
    }

    #[test]
    fn a_month_that_starts_inside_last_week_has_no_this_month() {
        // Tuesday 3 November: last week ran from Monday 26 October.
        let today = "2026-11-03";
        assert_eq!(group("2026-11-01", today), DateGroup::LastWeek);
        assert_eq!(group("2026-10-26", today), DateGroup::LastWeek);
        assert_eq!(group("2026-10-25", today), DateGroup::LastMonth);
        assert_eq!(group("2026-09-30", today), DateGroup::Older);
    }

    #[test]
    fn each_group_is_headed_by_its_count() {
        let messages = [
            message("a", "2026-10-28"),
            message("b", "2026-10-28"),
            message("c", "2026-10-20"),
        ];

        let rows = message_rows(&messages, &BTreeSet::new(), &now());

        assert_eq!(
            rows,
            [
                MessageRow::Header {
                    group: DateGroup::Today,
                    count: 2,
                    collapsed: false
                },
                MessageRow::Message(0),
                MessageRow::Message(1),
                MessageRow::Header {
                    group: DateGroup::LastWeek,
                    count: 1,
                    collapsed: false
                },
                MessageRow::Message(2),
            ]
        );
    }

    #[test]
    fn a_collapsed_group_shows_only_its_header() {
        let messages = [message("a", "2026-10-28"), message("b", "2026-10-28")];
        let collapsed = BTreeSet::from([DateGroup::Today]);

        let rows = message_rows(&messages, &collapsed, &now());

        assert_eq!(
            rows,
            [MessageRow::Header {
                group: DateGroup::Today,
                count: 2,
                collapsed: true
            }]
        );
    }
}
