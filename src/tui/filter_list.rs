//! A list picked from by typing part of a name: what a picker's filter holds,
//! which of the choices is picked, and the keys for both.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::input::TextInput;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterList {
    pub filter: TextInput,
    /// Which of the choices shown is picked.
    pub selected: usize,
}

impl FilterList {
    /// Whether nothing, or nothing but spaces, is typed.
    pub fn is_unfiltered(&self) -> bool {
        self.filter.text().trim().is_empty()
    }

    /// Whether `name` has what is typed in it, whatever its case and however
    /// many spaces are typed around it.
    pub fn matches(&self, name: &str) -> bool {
        let filter = self.filter.text().trim().to_lowercase();
        name.to_lowercase().contains(&filter)
    }

    /// ↑ ↓ Tab and Ctrl-N/P pick among `choices` shown, stopping at the ends;
    /// other keys edit the filter, which starts the picking over.
    pub fn key(&mut self, key: KeyEvent, choices: usize) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Down | KeyCode::Tab => self.step(1, choices),
            KeyCode::Char('n') if ctrl => self.step(1, choices),
            KeyCode::Up | KeyCode::BackTab => self.step(-1, choices),
            KeyCode::Char('p') if ctrl => self.step(-1, choices),
            _ => {
                let before = self.filter.text().to_owned();
                if self.filter.key(key) && self.filter.text() != before {
                    self.selected = 0;
                }
            }
        }
    }

    fn step(&mut self, delta: isize, choices: usize) {
        let last = choices.saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> FilterList {
        let mut list = FilterList::default();
        for c in text.chars() {
            list.key(KeyEvent::from(KeyCode::Char(c)), 0);
        }
        list
    }

    #[test]
    fn a_name_matches_whatever_its_case() {
        assert!(typed("rec").matches("Receipts"));
        assert!(!typed("rec").matches("Inbox"));
    }

    #[test]
    fn spaces_around_what_is_typed_do_not_count() {
        assert!(typed(" rec ").matches("Receipts"));
        assert!(typed("  ").is_unfiltered());
        assert!(typed("  ").matches("Inbox"));
    }

    #[test]
    fn spaces_inside_what_is_typed_do_count() {
        assert!(typed("my f").matches("my file.txt"));
        assert!(!typed("my f").matches("myfile.txt"));
    }

    #[test]
    fn picking_stops_at_the_ends() {
        let mut list = FilterList::default();

        list.key(KeyEvent::from(KeyCode::Up), 3);
        assert_eq!(list.selected, 0);

        for _ in 0..5 {
            list.key(KeyEvent::from(KeyCode::Down), 3);
        }
        assert_eq!(list.selected, 2);
    }

    #[test]
    fn tab_and_ctrl_n_and_p_pick_too() {
        let mut list = FilterList::default();
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);

        list.key(KeyEvent::from(KeyCode::Tab), 3);
        list.key(ctrl('n'), 3);
        assert_eq!(list.selected, 2);

        list.key(ctrl('p'), 3);
        list.key(KeyEvent::from(KeyCode::BackTab), 3);
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn typing_starts_the_picking_over() {
        let mut list = FilterList::default();
        list.key(KeyEvent::from(KeyCode::Down), 3);

        list.key(KeyEvent::from(KeyCode::Char('r')), 3);

        assert_eq!(list.selected, 0);
    }

    #[test]
    fn a_key_that_changes_nothing_keeps_the_pick() {
        let mut list = FilterList::default();
        list.key(KeyEvent::from(KeyCode::Down), 3);

        list.key(KeyEvent::from(KeyCode::Left), 3);

        assert_eq!(list.selected, 1);
    }
}
