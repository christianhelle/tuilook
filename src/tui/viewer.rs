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
    /// Copy the picked or yanked text to the clipboard.
    Copy(String),
    /// The viewer has no use for the key in normal mode, so the pane may.
    Unhandled,
}

impl Viewer {
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
        let picking = self.mode() != VimMode::Normal;
        if !reads(key, picking) {
            if picking {
                return Request::None;
            }
            // Drop a half typed command, such as the y of yy
            vim::to_normal(&mut self.editor);
            return Request::Unhandled;
        }
        match vim::key(&mut self.editor, key, page, &mut Register::default()) {
            Outcome::Yanked(text) => Request::Copy(text),
            Outcome::Unhandled | Outcome::Leave => Request::Unhandled,
            Outcome::Handled => Request::None,
        }
    }
}

/// Whether a key only moves, picks or yanks, leaving the text as it is.
/// Out of a visual mode, ← and → are left to the pane, to show the next or
/// previous mail.
fn reads(key: KeyEvent, picking: bool) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return matches!(key.code, KeyCode::Char('d' | 'u' | 'f' | 'b'));
    }
    match key.code {
        KeyCode::Char(c) => c.is_ascii_digit() || "hjklwbe^$gGvVy".contains(c),
        KeyCode::Left | KeyCode::Right => picking,
        KeyCode::Up
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
    fn v_picks_from_the_cursor_and_y_copies_it() {
        let mut viewer = viewing(
            "From: Ada
Dear Charles,
The engine",
        );

        let request = keys(&mut viewer, "jvwey");

        assert_eq!(request, Request::Copy("Dear Charles".into()));
        assert_eq!(viewer.mode(), VimMode::Normal);
    }

    #[test]
    fn yy_copies_the_line_in_normal_mode() {
        let mut viewer = viewing(
            "From: Ada
Dear Charles,",
        );

        assert_eq!(
            keys(&mut viewer, "jyy"),
            Request::Copy("Dear Charles,".into())
        );
    }

    #[test]
    fn keys_it_has_no_use_for_in_normal_mode_are_left_to_the_pane() {
        let mut viewer = viewing("Dear Charles,");

        for key in "dxiq ⎋".chars() {
            assert_eq!(
                keys(&mut viewer, &key.to_string()),
                Request::Unhandled,
                "{key}"
            );
        }
        for code in [KeyCode::Left, KeyCode::Right, KeyCode::Tab, KeyCode::Enter] {
            assert_eq!(viewer.key(KeyEvent::from(code), 10), Request::Unhandled);
        }
        assert_eq!(
            keys(&mut viewer, "0y$"),
            Request::Copy("Dear Charles,".into())
        );
    }

    #[test]
    fn keys_that_would_change_the_text_do_nothing_while_picking() {
        let mut viewer = viewing("Dear Charles,");

        assert_eq!(keys(&mut viewer, "vldxcpPr~"), Request::None);
        let request = keys(&mut viewer, "$y");

        assert_eq!(request, Request::Copy("Dear Charles,".into()));
    }

    #[test]
    fn esc_or_v_again_stops_picking() {
        let mut viewer = viewing("a");

        assert_eq!(keys(&mut viewer, "v⎋"), Request::None);
        assert_eq!(viewer.mode(), VimMode::Normal);
        keys(&mut viewer, "vv");
        assert_eq!(viewer.mode(), VimMode::Normal);
        keys(&mut viewer, "Vv");
        assert_eq!(viewer.mode(), VimMode::Visual);
    }
}
