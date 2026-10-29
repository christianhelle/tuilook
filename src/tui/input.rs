//! A single line of editable text.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => self.insert(c),
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
}
