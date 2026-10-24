//! Sample mailbox served by a [`FakeBackend`] for `tuilook --demo`.

use jiff::Timestamp;
use tuilook_core::fake::FakeBackend;
use tuilook_model::{Body, BodyType, Folder, FolderId, Importance, Message, MessageId, WellKnown};

pub fn backend() -> FakeBackend {
    let folders = vec![
        folder("inbox", "Inbox", Some(WellKnown::Inbox), 2, 3),
        folder("drafts", "Drafts", Some(WellKnown::Drafts), 0, 0),
        folder("sent", "Sent Items", Some(WellKnown::SentItems), 0, 1),
        folder("projects", "Projects", None, 0, 1),
    ];
    let inbox = vec![
        message(
            "m1",
            "inbox",
            "Ada Lovelace",
            "Notes on the analytical engine",
            "Attached are my notes on Menabrea's paper.",
            "2026-10-24T08:15:00Z",
            false,
        ),
        message(
            "m2",
            "inbox",
            "Grace Hopper",
            "Compiler draft",
            "First pass of the A-0 compiler is ready for review.",
            "2026-10-23T16:40:00Z",
            false,
        ),
        message(
            "m3",
            "inbox",
            "Alan Turing",
            "Lunch on Friday?",
            "Shall we continue the discussion on computable numbers?",
            "2026-10-22T11:05:00Z",
            true,
        ),
    ];
    let sent = vec![message(
        "m4",
        "sent",
        "Me",
        "Re: Compiler draft",
        "Looks great, a few comments inline.",
        "2026-10-23T17:02:00Z",
        true,
    )];
    let projects = vec![message(
        "m5",
        "projects",
        "Linus Torvalds",
        "Just a hobby",
        "Won't be big and professional like gnu.",
        "2026-10-20T09:00:00Z",
        true,
    )];
    let bodies = [&inbox, &sent, &projects]
        .into_iter()
        .flatten()
        .map(|m| {
            let html = format!(
                r#"<p>Hi,</p><p>{}</p><p>More at <a href="https://example.com/{}">example.com</a>.</p>"#,
                m.preview, m.id.0
            );
            let body = Body {
                content_type: BodyType::Html,
                content: html,
            };
            (m.id.clone(), body)
        })
        .collect();
    FakeBackend {
        folders,
        bodies,
        messages: [
            (FolderId("inbox".to_owned()), inbox),
            (FolderId("sent".to_owned()), sent),
            (FolderId("projects".to_owned()), projects),
        ]
        .into(),
        ..Default::default()
    }
}

fn folder(id: &str, name: &str, well_known: Option<WellKnown>, unread: u32, total: u32) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: None,
        name: name.to_owned(),
        well_known,
        unread,
        total,
    }
}

fn message(
    id: &str,
    folder: &str,
    from: &str,
    subject: &str,
    preview: &str,
    received_at: &str,
    is_read: bool,
) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId(folder.to_owned()),
        conversation_id: id.to_owned(),
        subject: subject.to_owned(),
        from_name: from.to_owned(),
        from_addr: format!(
            "{}@example.com",
            from.split(' ').next().unwrap_or(from).to_lowercase()
        ),
        received_at: received_at
            .parse::<Timestamp>()
            .expect("valid demo timestamp"),
        is_read,
        flagged: false,
        importance: Importance::Normal,
        has_attachments: false,
        preview: preview.to_owned(),
    }
}
