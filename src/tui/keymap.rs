//! Default vim-flavoured keybindings.

use crate::model::Action;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Turns key presses into actions, remembering a half-typed `gg`.
#[derive(Debug, Default)]
pub struct Keymap {
    pending_g: bool,
}

impl Keymap {
    pub fn action_for(&mut self, key: KeyEvent) -> Option<Action> {
        let pending_g = std::mem::take(&mut self.pending_g);
        if key.code == KeyCode::Char('g') && !key.modifiers.contains(KeyModifiers::CONTROL) {
            if pending_g {
                return Some(Action::Top);
            }
            self.pending_g = true;
            return None;
        }
        single_key(key)
    }
}

fn single_key(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        let action = match key.code {
            KeyCode::Char('c') => Action::Quit,
            KeyCode::Char('f') => Action::PageDown,
            KeyCode::Char('b') => Action::PageUp,
            KeyCode::Char('d') => Action::HalfPageDown,
            KeyCode::Char('u') => Action::HalfPageUp,
            KeyCode::Char('a') => Action::MarkAll,
            KeyCode::Char('r') => Action::Refresh,
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
        KeyCode::Tab => Action::FocusNextPane,
        KeyCode::BackTab => Action::FocusPrevPane,
        KeyCode::Enter => Action::Open,
        KeyCode::Char('l') | KeyCode::Right => Action::Expand,
        KeyCode::Char('h') | KeyCode::Left => Action::Collapse,
        KeyCode::Char('*') => Action::ToggleFavorite,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('z') => Action::ToggleZoom,
        KeyCode::Char('b') => Action::ToggleFolders,
        KeyCode::Char(' ') => Action::ToggleMark,
        KeyCode::Esc => Action::ClearMarks,
        KeyCode::Char('d') => Action::Delete,
        KeyCode::Char('D') => Action::DeletePermanently,
        KeyCode::F(5) => Action::Refresh,
        KeyCode::Char('?') => Action::Help,
        KeyCode::Char('c') => Action::Compose,
        KeyCode::Char('r') => Action::Reply,
        KeyCode::Char('R') => Action::ReplyAll,
        KeyCode::Char('f') => Action::Forward,
        KeyCode::Char('/') => Action::Search,
        KeyCode::Char('S') => Action::EditSignature,
        KeyCode::Char('v') => Action::Visual,
        KeyCode::Char('V') => Action::VisualLine,
        _ => return None,
    };
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Option<Action> {
        Keymap::default().action_for(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn vim_keys_and_arrows_move_selection() {
        assert_eq!(press(KeyCode::Char('j')), Some(Action::SelectNext));
        assert_eq!(press(KeyCode::Down), Some(Action::SelectNext));
        assert_eq!(press(KeyCode::Char('k')), Some(Action::SelectPrev));
        assert_eq!(press(KeyCode::Up), Some(Action::SelectPrev));
    }

    #[test]
    fn tab_and_shift_tab_move_focus_between_panes() {
        assert_eq!(press(KeyCode::Tab), Some(Action::FocusNextPane));
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
        assert_eq!(Keymap::default().action_for(key), Some(Action::Quit));
    }

    fn ctrl(c: char) -> Option<Action> {
        Keymap::default().action_for(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
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
        assert_eq!(Keymap::default().action_for(shift_g), Some(Action::Bottom));
        assert_eq!(press(KeyCode::End), Some(Action::Bottom));
        assert_eq!(press(KeyCode::Home), Some(Action::Top));
    }

    #[test]
    fn g_twice_jumps_to_the_top() {
        let mut keymap = Keymap::default();
        let g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);

        assert_eq!(keymap.action_for(g), None);
        assert_eq!(keymap.action_for(g), Some(Action::Top));
        assert_eq!(keymap.action_for(g), None, "a third g starts over");
    }

    #[test]
    fn g_followed_by_another_key_runs_that_key() {
        let mut keymap = Keymap::default();

        keymap.action_for(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        let next = keymap.action_for(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

        assert_eq!(next, Some(Action::SelectNext));
        let g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        assert_eq!(keymap.action_for(g), None, "the earlier g was dropped");
    }

    #[test]
    fn z_zooms_and_b_hides_the_folders() {
        assert_eq!(press(KeyCode::Char('z')), Some(Action::ToggleZoom));
        assert_eq!(press(KeyCode::Char('b')), Some(Action::ToggleFolders));
    }

    #[test]
    fn space_marks_ctrl_a_marks_all_and_esc_clears() {
        assert_eq!(press(KeyCode::Char(' ')), Some(Action::ToggleMark));
        assert_eq!(ctrl('a'), Some(Action::MarkAll));
        assert_eq!(press(KeyCode::Esc), Some(Action::ClearMarks));
    }

    #[test]
    fn d_deletes_and_shift_d_deletes_for_good() {
        assert_eq!(press(KeyCode::Char('d')), Some(Action::Delete));
        assert_eq!(press(KeyCode::Char('D')), Some(Action::DeletePermanently));
        assert_eq!(ctrl('d'), Some(Action::HalfPageDown), "Ctrl-D still pages");
    }

    #[test]
    fn f5_and_ctrl_r_refresh() {
        assert_eq!(press(KeyCode::F(5)), Some(Action::Refresh));
        assert_eq!(ctrl('r'), Some(Action::Refresh));
    }

    #[test]
    fn c_writes_r_replies_shift_r_replies_to_all_and_f_forwards() {
        assert_eq!(press(KeyCode::Char('c')), Some(Action::Compose));
        assert_eq!(press(KeyCode::Char('r')), Some(Action::Reply));
        assert_eq!(press(KeyCode::Char('R')), Some(Action::ReplyAll));
        assert_eq!(press(KeyCode::Char('f')), Some(Action::Forward));
        assert_eq!(ctrl('f'), Some(Action::PageDown), "Ctrl-F still pages");
    }

    #[test]
    fn l_and_right_expand_while_h_and_left_collapse() {
        assert_eq!(press(KeyCode::Right), Some(Action::Expand));
        assert_eq!(press(KeyCode::Char('l')), Some(Action::Expand));
        assert_eq!(press(KeyCode::Left), Some(Action::Collapse));
        assert_eq!(press(KeyCode::Char('h')), Some(Action::Collapse));
    }

    #[test]
    fn star_adds_or_removes_a_favorite() {
        // Typing * reports Shift on some keyboards.
        let star = KeyEvent::new(KeyCode::Char('*'), KeyModifiers::SHIFT);
        assert_eq!(
            Keymap::default().action_for(star),
            Some(Action::ToggleFavorite)
        );
    }

    #[test]
    fn slash_searches() {
        assert_eq!(press(KeyCode::Char('/')), Some(Action::Search));
    }

    #[test]
    fn shift_s_writes_the_signature() {
        let shift_s = KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT);
        assert_eq!(
            Keymap::default().action_for(shift_s),
            Some(Action::EditSignature)
        );
    }

    #[test]
    fn v_and_shift_v_pick_text_in_the_reader() {
        let shift_v = KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT);
        assert_eq!(press(KeyCode::Char('v')), Some(Action::Visual));
        assert_eq!(
            Keymap::default().action_for(shift_v),
            Some(Action::VisualLine)
        );
    }

    #[test]
    fn unbound_keys_do_nothing() {
        assert_eq!(press(KeyCode::Char('x')), None);
    }
}
