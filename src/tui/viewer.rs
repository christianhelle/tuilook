//! The reading pane's cursor, moved and picking text with Vim's keys.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{
    editor::{Editor, Pos},
    vim::{self, Outcome, Register, VimMode},
};

/// A read-only editor holding what the pane shows, with Vim's cursor.
#[derive(Debug, Default)]
pub struct Viewer {
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

impl Viewer {
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
        Viewer { editor }
    }

    /// Shows `lines`, keeping the cursor and mode as the new lines allow.
    pub fn show(&mut self, lines: &[String]) {
        if self.editor.lines() == lines {
            return;
        }
        let (row, column) = self.editor.cursor();
        let vim = std::mem::take(&mut self.editor.vim);
        self.editor.set_text(&lines.join(
            "
",
        ));
        self.editor.vim = vim;
        self.editor.move_to(row, column);
        self.editor.clamp_to_line();
    }

    /// Where the cursor is, as the line and the characters from its start.
    pub fn cursor(&self) -> Pos {
        self.editor.cursor()
    }

    /// Whether characters or whole lines are picked.
    pub fn mode(&self) -> VimMode {
        self.editor.vim.mode
    }

    /// The first and last picked places.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        self.editor.vim.selection(self.editor.cursor())
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

    fn picking(text: &str, row: usize) -> Viewer {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        Viewer::new(&lines, row, false)
    }

    /// Types each character as a key, `⎋` standing for Esc.
    fn keys(visual: &mut Viewer, keys: &str) -> Request {
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

    fn viewing(text: &str) -> Viewer {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut viewer = Viewer::default();
        viewer.show(&lines);
        viewer
    }

    #[test]
    fn keys_move_the_cursor_in_normal_mode() {
        let mut viewer = viewing(
            "From: Ada
Dear Charles,",
        );

        keys(&mut viewer, "jw");

        assert_eq!(viewer.cursor(), (1, 5));
        assert_eq!(viewer.mode(), VimMode::Normal);
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
