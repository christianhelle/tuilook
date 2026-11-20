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
    /// Whether a `g` waits for an `x`, to open the link under the cursor.
    g: bool,
}

/// What a key asks of the reading pane.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    None,
    /// Copy the picked or yanked text to the clipboard.
    Copy(String),
    /// Open the web address under the cursor in a browser.
    OpenUrl,
    /// The viewer has no use for the key in normal mode, so the pane may.
    Unhandled,
}

impl Viewer {
    /// Shows `lines`, keeping the cursor and mode as the new lines allow.
    pub fn show(&mut self, lines: &[String]) {
        if self.editor.lines() == lines {
            return;
        }
        self.g = false;
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

    /// Puts the cursor on a line and column, as near as the text allows.
    pub fn move_to(&mut self, row: usize, column: usize) {
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

    /// The editor holding what the pane shows, to read its mode and cursor.
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// The first row shown, when this many rows show at once, lines wrapped
    /// to `width`.
    pub fn scroll(&self, height: usize, width: usize) -> usize {
        self.editor.scroll(height, width)
    }

    /// Keeps the rows shown where they are, unless the cursor left them.
    pub fn follow(&mut self, height: usize, width: usize) {
        self.editor.follow(height, width);
    }

    /// The web address under the cursor, or the one a footnote reference under
    /// the cursor names.
    pub fn url_under_cursor(&self) -> Option<String> {
        let (row, column) = self.cursor();
        let lines = self.editor.lines();
        let line = lines.get(row)?;
        footnote_at(line, column)
            .and_then(|number| footnote_url(lines, &number))
            .or_else(|| url_at(line, column))
    }

    /// Acts on a key, moving `page` lines a page and yanking into `register`.
    pub fn key(&mut self, key: KeyEvent, page: usize, register: &mut Register) -> Request {
        let picking = self.mode() != VimMode::Normal;
        let pending_g = std::mem::take(&mut self.g);
        if !picking
            && pending_g
            && key.code == KeyCode::Char('x')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            // Drop the g Vim is waiting on, and leave the cursor where it is.
            vim::to_normal(&mut self.editor);
            return Request::OpenUrl;
        }
        if !reads(key, picking) {
            if picking {
                return Request::None;
            }
            // Drop a half typed command, such as the y of yy
            vim::to_normal(&mut self.editor);
            return Request::Unhandled;
        }
        let outcome = vim::key(&mut self.editor, key, page, register);
        if !picking
            && !pending_g
            && key.code == KeyCode::Char('g')
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && outcome == Outcome::Handled
        {
            self.g = true;
        }
        match outcome {
            Outcome::Yanked(text) => Request::Copy(text),
            Outcome::Unhandled | Outcome::Leave => Request::Unhandled,
            Outcome::Handled => Request::None,
        }
    }
}

/// A footnote reference `[n]` under the cursor, as `n`.
fn footnote_at(line: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    for start in 0..chars.len() {
        if chars[start] != '[' {
            continue;
        }
        let Some(end) = (start + 1..chars.len()).find(|&at| chars[at] == ']') else {
            continue;
        };
        if start > column || column > end {
            continue;
        }
        let number: String = chars[start + 1..end].iter().collect();
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            return Some(number);
        }
    }
    None
}

/// The address a footnote `number` names, as `[n]: address`.
fn footnote_url(lines: &[String], number: &str) -> Option<String> {
    let prefix = format!("[{number}]:");
    lines.iter().find_map(|line| {
        let rest = line.trim_start().strip_prefix(&prefix)?;
        let url = rest.trim();
        let url = url
            .strip_prefix('<')
            .and_then(|url| url.strip_suffix('>'))
            .unwrap_or(url);
        is_web_url(url).then(|| url.to_owned())
    })
}

/// A web address on `line` that the cursor at `column` is on.
fn url_at(line: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        if !starts_with(&chars, start, "http://") && !starts_with(&chars, start, "https://") {
            start += 1;
            continue;
        }
        if start > 0 && chars[start - 1].is_ascii_alphanumeric() {
            start += 1;
            continue;
        }
        let end = (start..chars.len())
            .find(|&at| {
                chars[at].is_whitespace() || matches!(chars[at], '<' | '>' | '"' | '\'' | '`')
            })
            .unwrap_or(chars.len());
        let end = trim_url_end(&chars, start, end);
        if start <= column && column < end {
            return Some(chars[start..end].iter().collect());
        }
        start = end.max(start + 1);
    }
    None
}

/// Whether `chars` at `start` spell `word`, whatever the case of its letters.
fn starts_with(chars: &[char], start: usize, word: &str) -> bool {
    let word: Vec<char> = word.chars().collect();
    chars.len() >= start + word.len()
        && chars[start..start + word.len()]
            .iter()
            .zip(&word)
            .all(|(c, w)| c.to_ascii_lowercase() == *w)
}

/// Where a web address ends, leaving out punctuation that is part of the
/// sentence or of a bracket around it.
fn trim_url_end(chars: &[char], start: usize, end: usize) -> usize {
    let mut end = end;
    while end > start && ".,;:!?".contains(chars[end - 1]) {
        end -= 1;
    }
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        while end > start
            && chars[end - 1] == close
            && chars[start..end].iter().filter(|&&c| c == close).count()
                > chars[start..end].iter().filter(|&&c| c == open).count()
        {
            end -= 1;
        }
    }
    end
}

/// Whether `url` is one a browser can open.
fn is_web_url(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty()
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
            request = visual.key(KeyEvent::from(code), 10, &mut Register::default());
        }
        request
    }

    fn viewing(text: &str) -> Viewer {
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let mut viewer = Viewer::default();
        viewer.show(&lines);
        viewer
    }

    /// Puts the cursor where `at` starts in `text`, and reads the address
    /// there.
    fn url_at_cursor(text: &str, at: &str) -> Option<String> {
        let mut viewer = viewing(text);
        let row = text.lines().position(|line| line.contains(at)).unwrap();
        let column = text.lines().nth(row).unwrap().find(at).unwrap();
        viewer.move_to(row, column);
        viewer.url_under_cursor()
    }

    #[test]
    fn a_web_address_under_the_cursor_opens() {
        assert_eq!(
            url_at_cursor("See https://example.com/docs for more", "https"),
            Some("https://example.com/docs".to_owned())
        );
        assert_eq!(
            url_at_cursor("An HTTP link: HTTP://EXAMPLE.COM/A", "HTTP://"),
            Some("HTTP://EXAMPLE.COM/A".to_owned())
        );
    }

    #[test]
    fn text_beside_a_web_address_has_nothing_to_open() {
        assert_eq!(url_at_cursor("See https://example.com/docs", "See"), None);
        assert_eq!(
            url_at_cursor("See https://example.com/docs", "docs"),
            Some("https://example.com/docs".to_owned()),
            "the end of the address is on it"
        );
        assert_eq!(
            url_at_cursor("See https://example.com/docs.", "example"),
            Some("https://example.com/docs".to_owned()),
            "a full stop after the address is not part of it"
        );
    }

    #[test]
    fn brackets_around_an_address_are_not_part_of_it() {
        assert_eq!(
            url_at_cursor("(see https://example.com/docs)", "example"),
            Some("https://example.com/docs".to_owned())
        );
        assert_eq!(
            url_at_cursor("https://en.wikipedia.org/wiki/Foo_(bar)", "wikipedia"),
            Some("https://en.wikipedia.org/wiki/Foo_(bar)".to_owned()),
            "a bracket the address opened is part of it"
        );
    }

    #[test]
    fn a_footnote_reference_opens_its_address() {
        let text = "See [the docs][1].\n\n[1]: https://example.com/docs";

        assert_eq!(
            url_at_cursor(text, "[1]"),
            Some("https://example.com/docs".to_owned())
        );
        assert_eq!(
            url_at_cursor(text, "[1]:"),
            Some("https://example.com/docs".to_owned())
        );
        assert_eq!(
            url_at_cursor(text, "https"),
            Some("https://example.com/docs".to_owned())
        );
    }

    #[test]
    fn only_web_addresses_open() {
        assert_eq!(
            url_at_cursor("Write to mailto:ada@example.com", "mailto"),
            None
        );
        assert_eq!(url_at_cursor("Fetch ftp://example.com/file", "ftp"), None);
        assert_eq!(url_at_cursor("A footnote [1] with no address", "[1]"), None);
    }

    #[test]
    fn a_yank_goes_to_the_register_to_put_elsewhere() {
        let mut viewer = viewing(
            "From: Ada
Dear Charles,",
        );
        let mut register = Register::default();

        for c in "jyy".chars() {
            viewer.key(KeyEvent::from(KeyCode::Char(c)), 10, &mut register);
        }

        assert_eq!(
            register,
            Register {
                text: "Dear Charles,".into(),
                linewise: true
            }
        );
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
    fn gx_asks_to_open_the_link_under_the_cursor() {
        let mut viewer = viewing("https://example.com/docs");

        assert_eq!(keys(&mut viewer, "gx"), Request::OpenUrl);
        assert_eq!(viewer.mode(), VimMode::Normal);
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
            let request = viewer.key(KeyEvent::from(code), 10, &mut Register::default());
            assert_eq!(request, Request::Unhandled);
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
