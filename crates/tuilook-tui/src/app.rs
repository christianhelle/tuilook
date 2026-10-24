use tuilook_model::Action;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Folders,
    Messages,
    Reader,
}

/// Everything the UI shows. Rendering is a pure function of this state.
#[derive(Debug, Default)]
pub struct AppState {
    pub focus: Pane,
    pub should_quit: bool,
}

impl AppState {
    pub fn update(&mut self, action: Action) {
        match action {
            Action::FocusNextPane => self.focus = self.focus.next(),
            Action::FocusPrevPane => self.focus = self.focus.prev(),
            Action::Quit => self.should_quit = true,
            _ => {}
        }
    }
}

impl Pane {
    fn next(self) -> Self {
        match self {
            Self::Folders => Self::Messages,
            Self::Messages => Self::Reader,
            Self::Reader => Self::Folders,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Folders => Self::Reader,
            Self::Messages => Self::Folders,
            Self::Reader => Self::Messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_stops_the_app() {
        let mut app = AppState::default();

        app.update(Action::Quit);

        assert!(app.should_quit);
    }

    #[test]
    fn focus_cycles_forward_through_panes() {
        let mut app = AppState::default();

        app.update(Action::FocusNextPane);
        assert_eq!(app.focus, Pane::Messages);
        app.update(Action::FocusNextPane);
        assert_eq!(app.focus, Pane::Reader);
        app.update(Action::FocusNextPane);
        assert_eq!(app.focus, Pane::Folders);
    }

    #[test]
    fn focus_cycles_backward_through_panes() {
        let mut app = AppState::default();

        app.update(Action::FocusPrevPane);
        assert_eq!(app.focus, Pane::Reader);
        app.update(Action::FocusPrevPane);
        assert_eq!(app.focus, Pane::Messages);
    }
}
