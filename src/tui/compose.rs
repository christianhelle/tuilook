//! Writing a message: the header fields, and a body edited vim style.

use crate::model::{ComposeKind, Draft, Message, parse_addresses};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{
    editor::Editor,
    input::TextInput,
    vim::{self, Outcome, Register, VimMode},
};

/// The part of the message keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The account it is sent from, picked from several on a new message.
    From,
    To,
    Cc,
    Subject,
    Body,
}

impl Field {
    fn next(self) -> Self {
        match self {
            Field::From => Field::To,
            Field::To => Field::Cc,
            Field::Cc => Field::Subject,
            Field::Subject => Field::Body,
            Field::Body => Field::From,
        }
    }

    fn prev(self) -> Self {
        match self {
            Field::From => Field::Body,
            Field::To => Field::From,
            Field::Cc => Field::To,
            Field::Subject => Field::Cc,
            Field::Body => Field::Subject,
        }
    }
}

/// What a key asks of the app, beyond editing the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Send(Draft),
    /// Close the message; nothing in it has changed.
    Close,
    /// Close the message once the user agrees to lose what they wrote.
    ConfirmDiscard,
    Help,
    /// The message cannot be sent as it is, for this reason.
    Invalid(String),
}

/// A message being written.
#[derive(Debug)]
pub struct Compose {
    pub kind: ComposeKind,
    pub to: TextInput,
    pub cc: TextInput,
    pub subject: TextInput,
    pub body: Editor,
    pub field: Field,
    /// Whether the message has gone to core to send, so keys wait.
    pub sending: bool,
    /// The account it is sent from.
    pub account: usize,
    /// Each account's address and signature, to send from.
    senders: Vec<(String, String)>,
    /// The message as it was opened, to tell whether anything was written.
    opened: Draft,
}

impl Compose {
    /// Starts a message. A reply or forward needs the `original` it answers;
    /// a reply starts in the body, ready to type.
    pub fn new(kind: ComposeKind, original: Option<&Message>) -> Self {
        let subject = original.map_or("", |m| m.subject.as_str());
        let (to, subject, field) = match &kind {
            ComposeKind::New => (String::new(), String::new(), Field::To),
            ComposeKind::Reply(_) | ComposeKind::ReplyAll(_) => (
                original.map(|m| m.from_addr.clone()).unwrap_or_default(),
                prefixed("RE:", subject),
                Field::Body,
            ),
            ComposeKind::Forward(_) => (String::new(), prefixed("FW:", subject), Field::To),
        };
        let mut compose = Compose {
            kind,
            to: TextInput::from(to.as_str()),
            cc: TextInput::default(),
            subject: TextInput::from(subject.as_str()),
            body: Editor::default(),
            field,
            sending: false,
            account: 0,
            senders: vec![],
            opened: Draft {
                kind: ComposeKind::New,
                to: vec![],
                cc: vec![],
                subject: String::new(),
                body: String::new(),
            },
        };
        if field == Field::Body {
            compose.body.vim.mode = VimMode::Insert;
        }
        compose.opened = compose.draft();
        compose
    }

    /// Puts `signature` below an empty first line of the body, ready to
    /// type above it. Nothing is added for an empty signature.
    pub fn with_signature(mut self, signature: &str) -> Self {
        if !signature.is_empty() {
            self.start_with(signature);
        }
        self
    }

    /// Makes the body `signature` below an empty first line, or nothing for
    /// an empty signature, as if the message had just been opened.
    fn start_with(&mut self, signature: &str) {
        let mode = self.body.vim.mode;
        let text = if signature.is_empty() {
            String::new()
        } else {
            format!(
                "

{signature}"
            )
        };
        self.body.set_text(&text);
        self.body.move_to(0, 0);
        self.body.vim.mode = mode;
        self.opened = self.draft();
    }

    /// Sends from `account` of `senders`, each an address and its signature,
    /// starting with that account's signature.
    pub fn with_senders(self, senders: Vec<(String, String)>, account: usize) -> Self {
        let signature = senders.get(account).map_or("", |(_, s)| s.as_str());
        let mut compose = self.with_signature(signature);
        compose.account = account;
        compose.senders = senders;
        compose
    }

    /// The address it is sent from, shown when there are several to send from.
    pub fn from(&self) -> Option<&str> {
        let several = self.senders.len() > 1;
        let (address, _) = self.senders.get(self.account).filter(|_| several)?;
        Some(address)
    }

    /// Whether the account it is sent from can be changed: a reply or
    /// forward is sent from the account the original is in.
    pub fn can_switch_sender(&self) -> bool {
        self.from().is_some() && self.kind == ComposeKind::New
    }

    fn next_field(&mut self) {
        self.field = self.field.next();
        if self.field == Field::From && !self.can_switch_sender() {
            self.field = self.field.next();
        }
    }

    fn prev_field(&mut self) {
        self.field = self.field.prev();
        if self.field == Field::From && !self.can_switch_sender() {
            self.field = self.field.prev();
        }
    }

    /// The message as it now stands.
    pub fn draft(&self) -> Draft {
        Draft {
            kind: self.kind.clone(),
            to: parse_addresses(self.to.text()),
            cc: parse_addresses(self.cc.text()),
            subject: self.subject.text().to_owned(),
            body: self.body.text(),
        }
    }

    /// Whether the original is quoted below the body when it is sent.
    pub fn quotes_original(&self) -> bool {
        self.kind != ComposeKind::New
    }

    /// Acts on a key, paging the body by `page` lines. Text is deleted or
    /// yanked into `register` and put from it.
    pub fn key(&mut self, key: KeyEvent, page: usize, register: &mut Register) -> Request {
        if self.sending {
            return Request::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if ctrl => return self.send(),
            KeyCode::Tab => self.next_field(),
            KeyCode::BackTab => self.prev_field(),
            _ if self.field == Field::Body => return self.body_key(key, page, register),
            _ => return self.header_key(key),
        }
        Request::None
    }

    fn body_key(&mut self, key: KeyEvent, page: usize, register: &mut Register) -> Request {
        let outcome = vim::key(&mut self.body, key, page, register);
        self.body.follow(page);
        match outcome {
            Outcome::Leave => self.close(),
            Outcome::Unhandled if key.code == KeyCode::Char('?') => Request::Help,
            Outcome::Handled | Outcome::Yanked(_) | Outcome::Unhandled => Request::None,
        }
    }

    fn header_key(&mut self, key: KeyEvent) -> Request {
        let input = match self.field {
            Field::To => &mut self.to,
            Field::Cc => &mut self.cc,
            Field::Subject => &mut self.subject,
            Field::From => return self.sender_key(key),
            Field::Body => unreachable!("the body has its own keys"),
        };
        match key.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Enter | KeyCode::Down => self.next_field(),
            KeyCode::Up => self.prev_field(),
            _ => {
                input.key(key);
            }
        }
        Request::None
    }

    fn sender_key(&mut self, key: KeyEvent) -> Request {
        match key.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Enter | KeyCode::Down => self.next_field(),
            KeyCode::Up => self.prev_field(),
            KeyCode::Right | KeyCode::Char(' ') => self.switch_sender(1),
            KeyCode::Left => self.switch_sender(self.senders.len() - 1),
            _ => {}
        }
        Request::None
    }

    /// Sends from the account `step` further round the senders.
    /// An untouched message takes that account's signature instead.
    fn switch_sender(&mut self, step: usize) {
        let untouched = self.draft() == self.opened;
        self.account = (self.account + step) % self.senders.len();
        if untouched {
            let (_, signature) = &self.senders[self.account];
            self.start_with(&signature.clone());
        }
    }

    fn close(&self) -> Request {
        if self.draft() == self.opened {
            Request::Close
        } else {
            Request::ConfirmDiscard
        }
    }

    fn send(&mut self) -> Request {
        let draft = self.draft();
        if draft.to.is_empty() {
            return Request::Invalid("Add someone to send it to".to_owned());
        }
        let addresses = draft.to.iter().chain(&draft.cc);
        if let Some(bad) = addresses.into_iter().find(|a| !a.contains('@')) {
            return Request::Invalid(format!("{bad} is not an email address"));
        }
        self.sending = true;
        Request::Send(draft)
    }
}

/// `subject` with `prefix` in front, unless it already starts with it.
fn prefixed(prefix: &str, subject: &str) -> String {
    let has = subject
        .get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix));
    if has {
        subject.to_owned()
    } else {
        format!("{prefix} {subject}")
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{FolderId, MessageId};

    use super::*;

    fn original() -> Message {
        Message {
            id: MessageId("m1".to_owned()),
            folder_id: FolderId("inbox".to_owned()),
            conversation_id: String::new(),
            subject: "Engine".to_owned(),
            from_name: "Ada".to_owned(),
            from_addr: "ada@example.com".to_owned(),
            received_at: jiff::Timestamp::UNIX_EPOCH,
            is_read: true,
            flagged: false,
            pinned: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    fn reply() -> ComposeKind {
        ComposeKind::Reply(MessageId("m1".to_owned()))
    }

    fn press(compose: &mut Compose, code: KeyCode) -> Request {
        compose.key(KeyEvent::from(code), 10, &mut Register::default())
    }

    /// Types `keys`, where ⎋ is Esc, ⇥ is Tab, ⏎ is Enter and ← → are arrows.
    fn typing(compose: &mut Compose, keys: &str) -> Request {
        let mut last = Request::None;
        for c in keys.chars() {
            let code = match c {
                '⎋' => KeyCode::Esc,
                '⇥' => KeyCode::Tab,
                '⏎' => KeyCode::Enter,
                '←' => KeyCode::Left,
                '→' => KeyCode::Right,
                c => KeyCode::Char(c),
            };
            last = compose.key(KeyEvent::from(code), 10, &mut Register::default());
        }
        last
    }

    fn ctrl_s(compose: &mut Compose) -> Request {
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        compose.key(ctrl_s, 10, &mut Register::default())
    }

    #[test]
    fn a_reply_goes_to_the_sender_with_re_in_front_and_starts_typing_the_body() {
        let compose = Compose::new(reply(), Some(&original()));

        assert_eq!(compose.to.text(), "ada@example.com");
        assert_eq!(compose.subject.text(), "RE: Engine");
        assert_eq!(compose.field, Field::Body);
        assert_eq!(compose.body.vim.mode, VimMode::Insert);
    }

    #[test]
    fn a_reply_to_a_reply_does_not_stack_re() {
        let mut message = original();
        message.subject = "Re: Engine".to_owned();

        let compose = Compose::new(reply(), Some(&message));

        assert_eq!(compose.subject.text(), "Re: Engine");
    }

    #[test]
    fn a_forward_starts_in_to_with_fw_in_front() {
        let compose = Compose::new(
            ComposeKind::Forward(MessageId("m1".to_owned())),
            Some(&original()),
        );

        assert_eq!(compose.to.text(), "");
        assert_eq!(compose.subject.text(), "FW: Engine");
        assert_eq!(compose.field, Field::To);
    }

    fn senders() -> Vec<(String, String)> {
        vec![
            ("me@outlook.com".to_owned(), String::new()),
            ("me@work.example".to_owned(), String::new()),
        ]
    }

    #[test]
    fn a_new_message_from_several_senders_moves_up_from_to_into_from() {
        let mut compose = Compose::new(ComposeKind::New, None).with_senders(senders(), 1);

        assert_eq!(compose.field, Field::To);
        assert_eq!(compose.from(), Some("me@work.example"));
        compose.key(
            KeyEvent::from(KeyCode::BackTab),
            10,
            &mut Register::default(),
        );
        assert_eq!(compose.field, Field::From);
        typing(&mut compose, "⇥");
        assert_eq!(compose.field, Field::To);
        typing(&mut compose, "⇥⇥⇥⇥");
        assert_eq!(compose.field, Field::From, "the body's tab wraps to From");
    }

    #[test]
    fn a_reply_or_a_single_sender_skips_from() {
        let mut reply = Compose::new(reply(), Some(&original())).with_senders(senders(), 0);
        let mut single =
            Compose::new(ComposeKind::New, None).with_senders(senders()[..1].to_vec(), 0);

        assert_eq!(reply.from(), Some("me@outlook.com"), "still shown");
        typing(&mut reply, "⇥");
        assert_eq!(reply.field, Field::To, "the body's tab skips From");
        assert_eq!(single.from(), None);
        single.key(
            KeyEvent::from(KeyCode::BackTab),
            10,
            &mut Register::default(),
        );
        assert_eq!(single.field, Field::Body);
        typing(&mut single, "⇥");
        assert_eq!(single.field, Field::To);
    }

    #[test]
    fn arrows_and_space_in_from_change_the_sender_round_the_accounts() {
        let mut senders = senders();
        senders.push(("me@home.example".to_owned(), String::new()));
        let mut compose = Compose::new(ComposeKind::New, None).with_senders(senders, 0);
        press(&mut compose, KeyCode::BackTab);

        typing(&mut compose, "→ →");
        assert_eq!(compose.account, 0, "wrapped past the last");
        typing(&mut compose, "←");
        assert_eq!(compose.from(), Some("me@home.example"));
        assert_eq!(compose.account, 2);
    }

    #[test]
    fn an_untouched_message_takes_the_new_senders_signature() {
        let senders = vec![
            ("me@outlook.com".to_owned(), String::new()),
            ("me@work.example".to_owned(), "Grace".to_owned()),
        ];
        let mut compose = Compose::new(ComposeKind::New, None).with_senders(senders, 0);
        press(&mut compose, KeyCode::BackTab);

        typing(&mut compose, "→");
        assert_eq!(
            compose.body.text(),
            "

Grace"
        );
        typing(&mut compose, "→");
        assert_eq!(compose.body.text(), "");
        assert_eq!(typing(&mut compose, "⎋"), Request::Close);
    }

    #[test]
    fn tab_and_enter_move_through_the_fields() {
        let mut compose = Compose::new(ComposeKind::New, None);

        typing(&mut compose, "ada@example.com⇥grace@example.com⏎Hello⇥");

        assert_eq!(compose.field, Field::Body);
        let draft = compose.draft();
        assert_eq!(draft.to, ["ada@example.com"]);
        assert_eq!(draft.cc, ["grace@example.com"]);
        assert_eq!(draft.subject, "Hello");
    }

    #[test]
    fn the_body_is_edited_vim_style() {
        let mut compose = Compose::new(reply(), Some(&original()));

        typing(&mut compose, "Hi there⎋bcwAda⎋");

        assert_eq!(compose.body.text(), "Hi Ada");
        assert_eq!(compose.body.vim.mode, VimMode::Normal);
    }

    #[test]
    fn ctrl_s_sends_the_message_and_then_waits() {
        let mut compose = Compose::new(reply(), Some(&original()));
        typing(&mut compose, "Agreed");

        let request = ctrl_s(&mut compose);

        assert_eq!(
            request,
            Request::Send(Draft {
                kind: reply(),
                to: vec!["ada@example.com".to_owned()],
                cc: vec![],
                subject: "RE: Engine".to_owned(),
                body: "Agreed".to_owned(),
            })
        );
        assert!(compose.sending);
        assert_eq!(typing(&mut compose, "x"), Request::None);
        assert_eq!(compose.body.text(), "Agreed", "keys wait while sending");
    }

    #[test]
    fn a_message_needs_someone_to_send_it_to() {
        let mut compose = Compose::new(ComposeKind::New, None);

        assert_eq!(
            ctrl_s(&mut compose),
            Request::Invalid("Add someone to send it to".to_owned())
        );
        typing(&mut compose, "ada");
        assert_eq!(
            ctrl_s(&mut compose),
            Request::Invalid("ada is not an email address".to_owned())
        );
        assert!(!compose.sending);
    }

    #[test]
    fn esc_closes_an_untouched_message_at_once() {
        let mut compose = Compose::new(ComposeKind::New, None);

        assert_eq!(typing(&mut compose, "⎋"), Request::Close);
    }

    #[test]
    fn esc_asks_before_losing_what_was_written() {
        let mut compose = Compose::new(reply(), Some(&original()));

        assert_eq!(typing(&mut compose, "Hi⎋"), Request::None, "to normal");
        assert_eq!(typing(&mut compose, "⎋"), Request::ConfirmDiscard);
    }

    #[test]
    fn a_signature_goes_below_where_the_message_is_typed() {
        let mut compose = Compose::new(reply(), Some(&original())).with_signature(
            "Grace
Navy",
        );

        typing(&mut compose, "Agreed");

        assert_eq!(
            compose.body.text(),
            "Agreed

Grace
Navy"
        );
        assert_eq!(compose.body.vim.mode, VimMode::Insert);
    }

    #[test]
    fn a_new_message_with_only_its_signature_closes_at_once() {
        let mut compose = Compose::new(ComposeKind::New, None).with_signature("Grace");

        assert_eq!(
            compose.body.text(),
            "

Grace"
        );
        assert_eq!(typing(&mut compose, "⎋"), Request::Close);
    }

    #[test]
    fn question_mark_in_normal_mode_asks_for_help() {
        let mut compose = Compose::new(reply(), Some(&original()));

        assert_eq!(typing(&mut compose, "⎋?"), Request::Help);
    }
}
