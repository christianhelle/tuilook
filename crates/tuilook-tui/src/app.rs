use tuilook_model::{Action, Folder, FolderId, Message, MessageId};

/// IO the event loop must perform after an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadFolder(FolderId),
    LoadBody(MessageId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Folders,
    Messages,
    Reader,
}

/// How much of each pane fits on screen, measured by the UI before each
/// update so paging moves by what the user can actually see.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewport {
    /// Folder rows that fit in the folder pane.
    pub folders: usize,
    /// Messages that fit in the message list.
    pub messages: usize,
    /// Body lines that fit in the reading pane.
    pub reader: usize,
    /// Lines the reading pane's content takes once wrapped.
    pub reader_lines: usize,
}

/// Everything the UI shows. Rendering is a pure function of this state.
#[derive(Debug, Default)]
pub struct AppState {
    pub focus: Pane,
    pub should_quit: bool,
    pub folders: Vec<Folder>,
    pub selected_folder: usize,
    pub messages: Vec<Message>,
    pub selected_message: usize,
    /// The selected message's body, once core has loaded it.
    pub body: Option<String>,
    /// Shown in the status bar until the next error replaces it.
    pub status: Option<String>,
    pub viewport: Viewport,
    /// Lines of the reading pane scrolled out of view at the top.
    pub reader_scroll: usize,
}

impl AppState {
    pub fn update(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::FocusNextPane => self.focus = self.focus.next(),
            Action::FocusPrevPane => self.focus = self.focus.prev(),
            Action::Quit => self.should_quit = true,
            Action::SelectNext => return self.move_selection(1),
            Action::SelectPrev => return self.move_selection(-1),
            Action::PageDown => return self.move_selection(self.page()),
            Action::PageUp => return self.move_selection(-self.page()),
            Action::HalfPageDown => return self.move_selection(self.page() / 2),
            Action::HalfPageUp => return self.move_selection(-self.page() / 2),
            Action::Top => return self.move_selection(-isize::MAX),
            Action::Bottom => return self.move_selection(isize::MAX),
            Action::Open => self.open(),
        }
        None
    }

    /// Shows a newly loaded folder's messages, selecting the first.
    pub fn set_messages(&mut self, messages: Vec<Message>) -> Option<Effect> {
        self.messages = messages;
        self.selected_message = 0;
        self.selection_changed()
    }

    /// Shows a newly synced folder tree, keeping the selected folder.
    pub fn set_folders(&mut self, folders: Vec<Folder>) -> Option<Effect> {
        let previous = self.current_folder().cloned();
        self.selected_folder = previous
            .as_ref()
            .and_then(|id| folders.iter().position(|f| &f.id == id))
            .unwrap_or(0);
        self.folders = folders;
        let current = self.current_folder().cloned();
        (current != previous)
            .then_some(current)
            .flatten()
            .map(Effect::LoadFolder)
    }

    pub fn current_folder(&self) -> Option<&FolderId> {
        self.folders.get(self.selected_folder).map(|f| &f.id)
    }

    /// Shows the current folder's messages after a sync, keeping the
    /// selected message when it is still there.
    pub fn refresh_messages(&mut self, messages: Vec<Message>) -> Option<Effect> {
        let selected = self.selected().map(|m| m.id.clone());
        match selected.and_then(|id| messages.iter().position(|m| m.id == id)) {
            Some(index) => {
                self.messages = messages;
                self.selected_message = index;
                None
            }
            None => self.set_messages(messages),
        }
    }

    pub fn show_error(&mut self, message: String) {
        self.status = Some(message);
    }

    /// Shows a body core loaded, unless the user has moved on.
    pub fn body_loaded(&mut self, message: MessageId, text: String) {
        if self.selected().is_some_and(|m| m.id == message) {
            self.body = Some(text);
        }
    }

    fn selected(&self) -> Option<&Message> {
        self.messages.get(self.selected_message)
    }

    /// Drops the old body and asks for the newly selected one.
    fn selection_changed(&mut self) -> Option<Effect> {
        self.body = None;
        self.reader_scroll = 0;
        self.selected().map(|m| Effect::LoadBody(m.id.clone()))
    }

    fn open(&mut self) {
        match self.focus {
            Pane::Folders if !self.folders.is_empty() => self.focus = Pane::Messages,
            Pane::Messages if !self.messages.is_empty() => self.focus = Pane::Reader,
            _ => {}
        }
    }

    /// Rows the focused pane shows; at least 2 so half a page still moves.
    fn page(&self) -> isize {
        let rows = match self.focus {
            Pane::Folders => self.viewport.folders,
            Pane::Messages => self.viewport.messages,
            Pane::Reader => self.viewport.reader,
        };
        rows.max(2) as isize
    }

    fn move_selection(&mut self, delta: isize) -> Option<Effect> {
        match self.focus {
            Pane::Folders => {
                let moved = step(&mut self.selected_folder, self.folders.len(), delta);
                moved.then(|| Effect::LoadFolder(self.folders[self.selected_folder].id.clone()))
            }
            Pane::Messages => {
                let moved = step(&mut self.selected_message, self.messages.len(), delta);
                if moved {
                    self.selection_changed()
                } else {
                    None
                }
            }
            Pane::Reader => {
                // Scroll until the last line is at the bottom of the pane.
                let max = self
                    .viewport
                    .reader_lines
                    .saturating_sub(self.viewport.reader);
                self.reader_scroll = self.reader_scroll.saturating_add_signed(delta).min(max);
                None
            }
        }
    }
}

/// Moves `index` by `delta` within `0..len`. Returns whether it moved.
fn step(index: &mut usize, len: usize, delta: isize) -> bool {
    let Some(last) = len.checked_sub(1) else {
        return false;
    };
    let next = index.saturating_add_signed(delta).min(last);
    let moved = next != *index;
    *index = next;
    moved
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

    fn folder(id: &str) -> Folder {
        Folder {
            id: FolderId(id.to_owned()),
            parent_id: None,
            name: id.to_owned(),
            well_known: None,
            unread: 0,
            total: 0,
        }
    }

    fn with_folders(ids: &[&str]) -> AppState {
        AppState {
            folders: ids.iter().map(|id| folder(id)).collect(),
            ..Default::default()
        }
    }

    fn message(id: &str) -> Message {
        Message {
            id: MessageId(id.to_owned()),
            folder_id: FolderId("a".to_owned()),
            conversation_id: String::new(),
            subject: id.to_owned(),
            from_name: String::new(),
            from_addr: String::new(),
            received_at: jiff::Timestamp::UNIX_EPOCH,
            is_read: false,
            flagged: false,
            importance: Default::default(),
            has_attachments: false,
            preview: String::new(),
        }
    }

    fn in_message_list(ids: &[&str]) -> AppState {
        let mut app = with_folders(&["a"]);
        app.set_messages(ids.iter().map(|id| message(id)).collect());
        app.focus = Pane::Messages;
        app
    }

    fn viewport(folders: usize, messages: usize, reader: usize) -> Viewport {
        Viewport {
            folders,
            messages,
            reader,
            reader_lines: 0,
        }
    }

    /// A message open in the reading pane, which shows `visible` of its
    /// `lines` lines.
    fn reading(visible: usize, lines: usize) -> AppState {
        let mut app = in_message_list(&["m0", "m1"]);
        app.focus = Pane::Reader;
        app.viewport = Viewport {
            reader: visible,
            reader_lines: lines,
            ..Default::default()
        };
        app
    }

    #[test]
    fn j_and_k_scroll_the_reading_pane_within_the_body() {
        let mut app = reading(10, 12);

        assert_eq!(app.update(Action::SelectNext), None);
        app.update(Action::SelectNext);
        app.update(Action::SelectNext);
        assert_eq!(app.reader_scroll, 2, "stops when the last line is in view");

        app.update(Action::SelectPrev);
        assert_eq!(app.reader_scroll, 1);
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn the_reading_pane_pages_and_jumps() {
        let mut app = reading(10, 100);

        app.update(Action::PageDown);
        assert_eq!(app.reader_scroll, 10);
        app.update(Action::HalfPageDown);
        assert_eq!(app.reader_scroll, 15);
        app.update(Action::Bottom);
        assert_eq!(app.reader_scroll, 90);
        app.update(Action::Top);
        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn a_body_that_fits_does_not_scroll() {
        let mut app = reading(10, 5);

        app.update(Action::PageDown);

        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn another_message_starts_at_the_top() {
        let mut app = reading(10, 100);
        app.update(Action::PageDown);
        app.focus = Pane::Messages;

        app.update(Action::SelectNext);

        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn paging_the_folder_list_moves_by_the_visible_rows() {
        let ids: Vec<String> = (0..30).map(|i| format!("f{i}")).collect();
        let mut app = with_folders(&ids.iter().map(String::as_str).collect::<Vec<_>>());
        app.viewport = viewport(10, 0, 0);

        let effect = app.update(Action::PageDown);
        assert_eq!(app.selected_folder, 10);
        assert_eq!(effect, Some(Effect::LoadFolder(FolderId("f10".to_owned()))));

        app.update(Action::HalfPageUp);
        assert_eq!(app.selected_folder, 5);
        app.update(Action::PageUp);
        assert_eq!(app.selected_folder, 0);
    }

    #[test]
    fn paging_the_message_list_requests_the_new_body() {
        let mut app = in_message_list(&["m0", "m1", "m2", "m3", "m4", "m5", "m6"]);
        app.viewport = viewport(0, 4, 0);

        assert_eq!(app.update(Action::HalfPageDown), load_body("m2"));
        assert_eq!(app.update(Action::PageDown), load_body("m6"));
        assert_eq!(app.update(Action::PageDown), None);
        assert_eq!(app.selected_message, 6);
    }

    #[test]
    fn top_and_bottom_jump_to_the_ends_of_the_list() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);

        assert_eq!(app.update(Action::Bottom), load_body("m2"));
        assert_eq!(app.update(Action::Bottom), None);
        assert_eq!(app.update(Action::Top), load_body("m0"));
    }

    #[test]
    fn paging_moves_at_least_one_row() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.viewport = viewport(0, 1, 0);

        assert_eq!(app.update(Action::HalfPageDown), load_body("m1"));
    }

    #[test]
    fn selection_moves_through_messages_when_the_list_has_focus() {
        let mut app = in_message_list(&["m1", "m2", "m3"]);

        app.update(Action::SelectNext);
        app.update(Action::SelectNext);
        app.update(Action::SelectNext);
        assert_eq!(app.selected_message, 2);
        assert_eq!(app.selected_folder, 0);

        app.update(Action::SelectPrev);
        assert_eq!(app.selected_message, 1);
    }

    #[test]
    fn opening_a_folder_focuses_its_message_list() {
        let mut app = with_folders(&["a"]);

        app.update(Action::Open);

        assert_eq!(app.focus, Pane::Messages);
    }

    #[test]
    fn opening_a_message_focuses_the_reading_pane() {
        let mut app = in_message_list(&["m1"]);

        app.update(Action::Open);

        assert_eq!(app.focus, Pane::Reader);
    }

    #[test]
    fn opening_in_an_empty_message_list_does_nothing() {
        let mut app = in_message_list(&[]);

        app.update(Action::Open);

        assert_eq!(app.focus, Pane::Messages);
    }

    fn load_body(id: &str) -> Option<Effect> {
        Some(Effect::LoadBody(MessageId(id.to_owned())))
    }

    #[test]
    fn moving_to_another_message_requests_its_body() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.body_loaded(MessageId("m1".to_owned()), "first".to_owned());

        assert_eq!(app.update(Action::SelectNext), load_body("m2"));
        assert_eq!(app.body, None);
        assert_eq!(app.update(Action::SelectNext), None);
    }

    #[test]
    fn loading_messages_requests_the_first_body() {
        let mut app = with_folders(&["a"]);

        assert_eq!(app.set_messages(vec![message("m1")]), load_body("m1"));
        assert_eq!(app.set_messages(vec![]), None);
    }

    #[test]
    fn a_body_is_shown_only_if_its_message_is_still_selected() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.update(Action::SelectNext);

        app.body_loaded(MessageId("m1".to_owned()), "stale".to_owned());
        assert_eq!(app.body, None);

        app.body_loaded(MessageId("m2".to_owned()), "fresh".to_owned());
        assert_eq!(app.body.as_deref(), Some("fresh"));
    }

    #[test]
    fn the_first_folder_tree_selects_and_loads_the_first_folder() {
        let mut app = AppState::default();

        let effect = app.set_folders(vec![folder("a"), folder("b")]);

        assert_eq!(effect, Some(Effect::LoadFolder(FolderId("a".to_owned()))));
        assert_eq!(app.current_folder(), Some(&FolderId("a".to_owned())));
    }

    #[test]
    fn a_refreshed_folder_tree_keeps_the_selected_folder() {
        let mut app = with_folders(&["a", "b"]);
        app.update(Action::SelectNext);

        let effect = app.set_folders(vec![folder("new"), folder("a"), folder("b")]);

        assert_eq!(effect, None);
        assert_eq!(app.selected_folder, 2);
    }

    #[test]
    fn refreshed_messages_keep_the_selected_message_and_its_body() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.update(Action::SelectNext);
        app.body_loaded(MessageId("m2".to_owned()), "body".to_owned());

        let effect = app.refresh_messages(vec![message("m0"), message("m1"), message("m2")]);

        assert_eq!(effect, None);
        assert_eq!(app.selected_message, 2);
        assert_eq!(app.body.as_deref(), Some("body"));
    }

    #[test]
    fn when_the_selected_message_disappears_the_first_is_selected() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.update(Action::SelectNext);

        let effect = app.refresh_messages(vec![message("m1")]);

        assert_eq!(effect, load_body("m1"));
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn loading_messages_selects_the_first() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.update(Action::SelectNext);

        app.set_messages(vec![message("m9")]);

        assert_eq!(app.selected_message, 0);
        assert_eq!(app.messages, [message("m9")]);
    }

    #[test]
    fn selecting_next_folder_loads_it() {
        let mut app = with_folders(&["a", "b", "c"]);

        let effect = app.update(Action::SelectNext);

        assert_eq!(app.selected_folder, 1);
        assert_eq!(effect, Some(Effect::LoadFolder(FolderId("b".to_owned()))));
    }

    #[test]
    fn selecting_previous_folder_loads_it() {
        let mut app = with_folders(&["a", "b", "c"]);
        app.selected_folder = 2;

        let effect = app.update(Action::SelectPrev);

        assert_eq!(app.selected_folder, 1);
        assert_eq!(effect, Some(Effect::LoadFolder(FolderId("b".to_owned()))));
    }

    #[test]
    fn folder_selection_stops_at_the_ends() {
        let mut app = with_folders(&["a", "b"]);

        assert_eq!(app.update(Action::SelectPrev), None);
        assert_eq!(app.selected_folder, 0);

        app.selected_folder = 1;
        assert_eq!(app.update(Action::SelectNext), None);
        assert_eq!(app.selected_folder, 1);
    }

    #[test]
    fn selecting_with_no_folders_does_nothing() {
        let mut app = AppState::default();

        assert_eq!(app.update(Action::SelectNext), None);
    }

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
