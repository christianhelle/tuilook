//! Default vim-flavoured keybindings.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tuilook_model::Action;

pub fn action_for(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        let action = match key.code {
            KeyCode::Char('c') => Action::Quit,
            KeyCode::Char('f') => Action::PageDown,
            KeyCode::Char('b') => Action::PageUp,
            KeyCode::Char('d') => Action::HalfPageDown,
            KeyCode::Char('u') => Action::HalfPageUp,
            _ => return None,
        };
        return Some(action);
    }
    let action = match key.code {
        KeyCode::Char('j') | KeyCode::Down => Action::SelectNext,
        KeyCode::Char('k') | KeyCode::Up => Action::SelectPrev,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::Char('G') | KeyCode::End => Action::Bottom,
        KeyCode::Home => Action::Top,
        KeyCode::Char('l') | KeyCode::Tab => Action::FocusNextPane,
        KeyCode::Char('h') | KeyCode::BackTab => Action::FocusPrevPane,
        KeyCode::Enter => Action::Open,
        KeyCode::Char('q') => Action::Quit,
        _ => return None,
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Option<Action> {
        action_for(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn vim_keys_and_arrows_move_selection() {
        assert_eq!(press(KeyCode::Char('j')), Some(Action::SelectNext));
        assert_eq!(press(KeyCode::Down), Some(Action::SelectNext));
        assert_eq!(press(KeyCode::Char('k')), Some(Action::SelectPrev));
        assert_eq!(press(KeyCode::Up), Some(Action::SelectPrev));
    }

    #[test]
    fn h_l_and_tab_move_focus_between_panes() {
        assert_eq!(press(KeyCode::Char('l')), Some(Action::FocusNextPane));
        assert_eq!(press(KeyCode::Tab), Some(Action::FocusNextPane));
        assert_eq!(press(KeyCode::Char('h')), Some(Action::FocusPrevPane));
        assert_eq!(press(KeyCode::BackTab), Some(Action::FocusPrevPane));
    }

    #[test]
    fn enter_opens_and_q_quits() {
        assert_eq!(press(KeyCode::Enter), Some(Action::Open));
        assert_eq!(press(KeyCode::Char('q')), Some(Action::Quit));
    }

    #[test]
    fn ctrl_c_quits() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(action_for(key), Some(Action::Quit));
    }

    fn ctrl(c: char) -> Option<Action> {
        action_for(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    #[test]
    fn page_keys_and_vim_scrolls_page() {
        assert_eq!(press(KeyCode::PageDown), Some(Action::PageDown));
        assert_eq!(press(KeyCode::PageUp), Some(Action::PageUp));
        assert_eq!(ctrl('f'), Some(Action::PageDown));
        assert_eq!(ctrl('b'), Some(Action::PageUp));
        assert_eq!(ctrl('d'), Some(Action::HalfPageDown));
        assert_eq!(ctrl('u'), Some(Action::HalfPageUp));
    }

    #[test]
    fn capital_g_home_and_end_jump_to_the_ends() {
        let shift_g = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(action_for(shift_g), Some(Action::Bottom));
        assert_eq!(press(KeyCode::End), Some(Action::Bottom));
        assert_eq!(press(KeyCode::Home), Some(Action::Top));
    }

    #[test]
    fn unbound_keys_do_nothing() {
        assert_eq!(press(KeyCode::Char('z')), None);
    }
}
