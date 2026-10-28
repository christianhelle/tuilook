//! Writing an account's signature, edited vim style like a message body.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{
    editor::Editor,
    vim::{self, Outcome, Register, VimMode},
};

/// What a key asks of the app, beyond editing the signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    /// Keep this as the account's signature.
    Save(String),
    /// Close the signature; nothing in it has changed.
    Close,
    /// Close the signature once the user agrees to lose the changes.
    ConfirmDiscard,
    Help,
}

/// An account's signature being written.
#[derive(Debug)]
pub struct SignatureEditor {
    /// The account it belongs to.
    pub account: usize,
    pub body: Editor,
    register: Register,
    /// The signature as it was opened, to tell whether anything changed.
    opened: String,
}

impl SignatureEditor {
    /// Starts editing `signature`, ready to type at its end.
    pub fn new(account: usize, signature: &str) -> Self {
        let mut body = Editor::default();
        body.set_text(signature);
        body.vim.mode = VimMode::Insert;
        SignatureEditor {
            account,
            opened: body.text(),
            body,
            register: Register::default(),
        }
    }

    /// The signature as it now stands, without trailing blank space.
    pub fn text(&self) -> String {
        self.body.text().trim_end().to_owned()
    }

    /// Acts on a key, paging by `page` lines.
    pub fn key(&mut self, key: KeyEvent, page: usize) -> Request {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Request::Save(self.text());
        }
        let outcome = vim::key(&mut self.body, key, page, &mut self.register);
        self.body.follow(page);
        match outcome {
            Outcome::Leave if self.body.text() == self.opened => Request::Close,
            Outcome::Leave => Request::ConfirmDiscard,
            Outcome::Unhandled if key.code == KeyCode::Char('?') => Request::Help,
            Outcome::Handled | Outcome::Yanked(_) | Outcome::Unhandled => Request::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `keys`, where ⎋ is Esc and ⏎ is Enter.
    fn typing(editor: &mut SignatureEditor, keys: &str) -> Request {
        let mut last = Request::None;
        for c in keys.chars() {
            let code = match c {
                '⎋' => KeyCode::Esc,
                '⏎' => KeyCode::Enter,
                c => KeyCode::Char(c),
            };
            last = editor.key(KeyEvent::from(code), 10);
        }
        last
    }

    fn ctrl_s(editor: &mut SignatureEditor) -> Request {
        editor.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), 10)
    }

    #[test]
    fn typing_and_ctrl_s_saves_the_signature() {
        let mut editor = SignatureEditor::new(1, "");

        typing(&mut editor, "Ada⏎Engines⏎⏎");

        assert_eq!(
            ctrl_s(&mut editor),
            Request::Save("Ada\nEngines".to_owned())
        );
    }

    #[test]
    fn typing_carries_on_after_the_signature_there_was() {
        let mut editor = SignatureEditor::new(0, "Ada");

        typing(&mut editor, " L.");

        assert_eq!(editor.text(), "Ada L.");
    }

    #[test]
    fn esc_closes_an_unchanged_signature_at_once() {
        let mut editor = SignatureEditor::new(0, "Ada");

        assert_eq!(typing(&mut editor, "⎋⎋"), Request::Close);
    }

    #[test]
    fn esc_asks_before_losing_changes() {
        let mut editor = SignatureEditor::new(0, "Ada");

        assert_eq!(typing(&mut editor, "!⎋⎋"), Request::ConfirmDiscard);
    }

    #[test]
    fn question_mark_in_normal_mode_asks_for_help() {
        let mut editor = SignatureEditor::new(0, "");

        assert_eq!(typing(&mut editor, "⎋?"), Request::Help);
    }
}
