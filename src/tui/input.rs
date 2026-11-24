//! A single line of editable text.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::word::{Class, class_of};

/// Text with a cursor, counted in characters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    cursor: usize,
}

/// The text, with the cursor after it.
impl From<&str> for TextInput {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }
}

impl TextInput {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the cursor is, in characters from the start.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The part of the text that fits in `width` characters with the cursor
    /// in view, scrolled as little as it takes, and the cursor's place in it.
    pub fn view(&self, width: usize) -> (String, usize) {
        let skip = self.cursor.saturating_sub(width.saturating_sub(1));
        let text = self.text.chars().skip(skip).take(width).collect();
        (text, self.cursor - skip)
    }

    /// Edits the text or moves the cursor for an editing key, and says
    /// whether `key` was one.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char(c) if !ctrl => self.insert(c),
            // Ctrl-Backspace deletes a word. Terminals that cannot send it
            // send Ctrl-H or Ctrl-W instead, so those do the same.
            KeyCode::Backspace if ctrl => self.delete_word_back(),
            KeyCode::Char('h' | 'w') if ctrl => self.delete_word_back(),
            KeyCode::Delete if ctrl => self.delete_word_forward(),
            KeyCode::Left if ctrl => self.word_left(),
            KeyCode::Right if ctrl => self.word_right(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            _ => return false,
        }
        true
    }

    pub fn insert(&mut self, c: char) {
        let at = self.byte_index(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.delete();
        }
    }

    /// Moves back to the start of the word before the cursor, as Windows'
    /// Ctrl-Left does.
    pub fn word_left(&mut self) {
        if let Some(at) = self.word_before(self.cursor) {
            self.cursor = at;
        }
    }

    /// Moves on to the start of the word after the cursor, as Windows'
    /// Ctrl-Right does.
    pub fn word_right(&mut self) {
        if let Some(at) = self.word_after(self.cursor) {
            self.cursor = at;
        }
    }

    /// Deletes from the cursor back to the start of the word before it, as
    /// Windows' Ctrl-Backspace does.
    pub fn delete_word_back(&mut self) {
        if let Some(at) = self.word_before(self.cursor) {
            self.cut(at, self.cursor);
            self.cursor = at;
        }
    }

    /// Deletes from the cursor on to the start of the word after it, as
    /// Windows' Ctrl-Delete does.
    pub fn delete_word_forward(&mut self) {
        if let Some(at) = self.word_after(self.cursor) {
            self.cut(self.cursor, at);
        }
    }

    /// The class of the character at `at`, or of the end of the text.
    fn class_at(&self, at: usize) -> Class {
        class_of(self.text.chars().nth(at))
    }

    /// Where the word before `at` starts, as Vim's `b` finds it, or `None`
    /// at the start of the text.
    fn word_before(&self, at: usize) -> Option<usize> {
        let mut here = at.checked_sub(1)?;
        while here > 0 && self.class_at(here) == Class::Space {
            here -= 1;
        }
        let class = self.class_at(here);
        while here > 0 && self.class_at(here - 1) == class {
            here -= 1;
        }
        Some(here)
    }

    /// Where the word after `at` starts, or `None` at the end of the text.
    fn word_after(&self, at: usize) -> Option<usize> {
        let len = self.len();
        let mut here = at;
        let class = self.class_at(at);
        if class != Class::Space {
            while here < len && self.class_at(here) == class {
                here += 1;
            }
        }
        while here < len && self.class_at(here) == Class::Space {
            here += 1;
        }
        (here != at).then_some(here)
    }

    /// Takes the characters from `from` up to `to` out of the text.
    fn cut(&mut self, from: usize, to: usize) {
        let (start, end) = (self.byte_index(from), self.byte_index(to));
        self.text.replace_range(start..end, "");
    }

    /// Deletes the character under the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let at = self.byte_index(self.cursor);
            self.text.remove(at);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.len();
    }

    /// Puts the cursor before a character, or at the end when there are fewer.
    pub fn move_to(&mut self, chars: usize) {
        self.cursor = chars.min(self.len());
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_index(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(index, _)| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_edit_at_the_cursor() {
        let mut input = TextInput::from("ac");

        for code in [
            KeyCode::Left,
            KeyCode::Char('b'),
            KeyCode::End,
            KeyCode::Backspace,
        ] {
            assert!(input.key(KeyEvent::from(code)));
        }

        assert_eq!(input.text(), "ab");
        assert!(!input.key(KeyEvent::from(KeyCode::Enter)));
    }

    #[test]
    fn a_long_text_scrolls_to_keep_the_cursor_in_view() {
        let mut input = TextInput::from("abcdef");

        assert_eq!(input.view(4), ("def".to_owned(), 3));
        input.home();
        assert_eq!(input.view(4), ("abcd".to_owned(), 0));
    }

    #[test]
    fn moving_the_cursor_stops_at_the_end_of_the_text() {
        let mut input = TextInput::default();
        "abc".chars().for_each(|c| input.insert(c));

        input.move_to(1);
        assert_eq!(input.cursor(), 1);
        input.move_to(10);
        assert_eq!(input.cursor(), 3);
    }

    /// Presses `key` with Ctrl held.
    fn ctrl_key(input: &mut TextInput, code: KeyCode) -> bool {
        input.key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    #[test]
    fn ctrl_left_and_right_move_a_word_at_a_time() {
        let mut input = TextInput::from("Hi there!");

        assert!(ctrl_key(&mut input, KeyCode::Left));
        assert_eq!(input.cursor(), 8, "on the ! it moved off");
        ctrl_key(&mut input, KeyCode::Left);
        assert_eq!(input.cursor(), 3);
        ctrl_key(&mut input, KeyCode::Left);
        assert_eq!(input.cursor(), 0);
        ctrl_key(&mut input, KeyCode::Left);
        assert_eq!(input.cursor(), 0, "stops at the start");

        ctrl_key(&mut input, KeyCode::Right);
        assert_eq!(input.cursor(), 3);
        ctrl_key(&mut input, KeyCode::Right);
        assert_eq!(input.cursor(), 8);
        ctrl_key(&mut input, KeyCode::Right);
        assert_eq!(input.cursor(), 9, "the end of the text");
    }

    #[test]
    fn ctrl_backspace_deletes_a_word() {
        let mut input = TextInput::from("Hi there!");

        assert!(ctrl_key(&mut input, KeyCode::Backspace));
        assert_eq!(input.text(), "Hi there");
        ctrl_key(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "Hi ");
        ctrl_key(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "", "as `b` does, past the space too");
        ctrl_key(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "", "and nothing at the start");
    }

    #[test]
    fn ctrl_h_and_ctrl_w_delete_a_word_too_and_type_nothing() {
        for c in ['h', 'w'] {
            let mut input = TextInput::from("Hi there!");

            assert!(
                ctrl_key(&mut input, KeyCode::Char(c)),
                "Ctrl-{c} is an editing key"
            );

            assert_eq!(input.text(), "Hi there", "Ctrl-{c}");
        }
    }

    #[test]
    fn ctrl_delete_deletes_the_word_after_the_cursor() {
        let mut input = TextInput::from("Hi there!");
        input.home();

        assert!(ctrl_key(&mut input, KeyCode::Delete));
        assert_eq!(input.text(), "there!", "as `w` finds the word after");
        assert_eq!(input.cursor(), 0);

        ctrl_key(&mut input, KeyCode::Delete);
        assert_eq!(input.text(), "!", "the ! is a run of its own");
        ctrl_key(&mut input, KeyCode::Delete);
        assert_eq!(input.text(), "", "and the last run goes too");
    }

    #[test]
    fn the_word_keys_do_nothing_to_empty_text() {
        let mut input = TextInput::default();

        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Char('h'),
            KeyCode::Char('w'),
        ] {
            ctrl_key(&mut input, code);

            assert_eq!(input.text(), "", "{code:?}");
            assert_eq!(input.cursor(), 0, "{code:?}");
        }
    }

    #[test]
    fn the_word_keys_leave_punctuation_runs_whole() {
        let mut input = TextInput::from("ada@example.com");

        ctrl_key(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "ada@example.");
        ctrl_key(&mut input, KeyCode::Backspace);
        assert_eq!(input.text(), "ada@example");
        ctrl_key(&mut input, KeyCode::Backspace);

        assert_eq!(input.text(), "ada@");
    }
}
