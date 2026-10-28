//! Picking text in the reading pane with Vim's visual mode, to copy it.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{
    editor::Editor,
    vim::{self, Outcome, Register, VimMode},
};

/// Text being picked, in a read-only editor holding what the pane shows.
#[derive(Debug)]
pub struct Visual {
    editor: Editor,
}

/// What a key asks of the reading pane.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    None,
    /// Copy the picked text to the clipboard.
    Copy(String),
    /// Stop picking.
    Close,
}

impl Visual {
    /// Starts picking `lines` at the start of line `row`, whole lines when
    /// `line`.
    pub fn new(lines: &[String], row: usize, line: bool) -> Self {
        let mut editor = Editor::default();
        editor.set_text(&lines.join("\n"));
        editor.move_to(row, 0);
        let start = if line { 'V' } else { 'v' };
        vim::key(
            &mut editor,
            KeyEvent::from(KeyCode::Char(start)),
            1,
            &mut Register::default(),
        );
        Visual { editor }
    }

    /// Acts on a key, moving `page` lines a page.
    pub fn key(&mut self, key: KeyEvent, page: usize) -> Request {
        if !reads(key) {
            return Request::None;
        }
        match vim::key(&mut self.editor, key, page, &mut Register::default()) {
            Outcome::Yanked(text) => Request::Copy(text),
            _ if self.editor.vim.mode == VimMode::Normal => Request::Close,
            _ => Request::None,
        }
    }
}

/// Whether a key only moves, picks or yanks, leaving the text as it is.
fn reads(key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('d' | 'u' | 'f' | 'b'));
    }
    match key.code {
        KeyCode::Char(c) => c.is_ascii_digit() || "hjklwbe^$gGvVy ".contains(c),
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown
        | KeyCode::Backspace
        | KeyCode::Esc => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picking(text: &str, row: usize) -> Visual {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        Visual::new(&lines, row, false)
    }

    /// Types each character as a key, `⎋` standing for Esc.
    fn keys(visual: &mut Visual, keys: &str) -> Request {
        let mut request = Request::None;
        for c in keys.chars() {
            let code = match c {
                '⎋' => KeyCode::Esc,
                c => KeyCode::Char(c),
            };
            request = visual.key(KeyEvent::from(code), 10);
        }
        request
    }

    #[test]
    fn y_copies_the_characters_picked_from_where_it_started() {
        let mut visual = picking("From: Ada\nDear Charles,\nThe engine", 1);

        let request = keys(&mut visual, "wey");

        assert_eq!(request, Request::Copy("Dear Charles".into()));
    }

    #[test]
    fn keys_that_would_change_the_text_do_nothing() {
        let mut visual = picking("Dear Charles,", 0);

        assert_eq!(keys(&mut visual, "ldxcpPr~"), Request::None);
        let request = keys(&mut visual, "$y");

        assert_eq!(request, Request::Copy("Dear Charles,".into()));
    }

    #[test]
    fn esc_or_v_again_stops_picking() {
        assert_eq!(keys(&mut picking("a", 0), "⎋"), Request::Close);
        assert_eq!(keys(&mut picking("a", 0), "v"), Request::Close);
        assert_eq!(keys(&mut picking("a", 0), "Vv"), Request::None);
    }
}
