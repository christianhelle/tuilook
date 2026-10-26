use std::collections::BTreeSet;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tuilook_model::{Action, ComposeKind, Draft, Folder, FolderId, Message, MessageId};

use crate::{
    compose::{Compose, Request},
    keymap::Keymap,
};

/// IO the event loop must perform after an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadFolder(FolderId),
    LoadBody(MessageId),
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
    },
    /// Sync every folder.
    Sync,
    Send(Draft),
}

/// A question the user must answer before anything else happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
    },
    /// Close the message being written, losing what is in it.
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Pane {
    #[default]
    Folders,
    Messages,
    Reader,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    Click,
    ScrollUp,
    ScrollDown,
}

/// Where the mouse is: a pane, and the folder or message row under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub pane: Pane,
    pub item: Option<usize>,
}

/// Rows or lines the mouse wheel moves at a time.
const WHEEL_STEP: isize = 3;

/// A line for the status bar, shown until the next one replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
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
    /// Body lines that fit while writing a message.
    pub compose: usize,
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
    pub status: Option<Status>,
    pub viewport: Viewport,
    /// Lines of the reading pane scrolled out of view at the top.
    pub reader_scroll: usize,
    /// Whether the focused pane fills the screen.
    pub zoomed: bool,
    pub folders_hidden: bool,
    /// Messages picked for acting on together, such as deleting.
    pub marked: BTreeSet<MessageId>,
    pub confirm: Option<Confirm>,
    pub show_help: bool,
    /// A message being written, which takes over the screen.
    pub compose: Option<Compose>,
}

impl AppState {
    /// The pane filling the screen, which follows the focus while zoomed.
    pub fn zoomed_pane(&self) -> Option<Pane> {
        self.zoomed.then_some(self.focus)
    }

    /// Handles a key press: an open dialog takes it first, then the keymap.
    pub fn on_key(&mut self, key: KeyEvent, keymap: &mut Keymap) -> Option<Effect> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return None;
        }
        if self.show_help {
            self.show_help = false;
            return None;
        }
        if let Some(confirm) = self.confirm.take() {
            // y or Enter agrees; any other key cancels and does nothing else.
            let agreed = matches!(key.code, KeyCode::Char('y') | KeyCode::Enter);
            return agreed.then(|| self.confirmed(confirm)).flatten();
        }
        if let Some(compose) = &mut self.compose {
            match compose.key(key, self.viewport.compose) {
                Request::None => {}
                Request::Send(draft) => {
                    self.show_info("Sending…");
                    return Some(Effect::Send(draft));
                }
                Request::Close => self.compose = None,
                Request::ConfirmDiscard => self.confirm = Some(Confirm::Discard),
                Request::Help => self.show_help = true,
                Request::Invalid(why) => self.show_error(why),
            }
            return None;
        }
        keymap
            .action_for(key)
            .and_then(|action| self.update(action))
    }

    /// Handles the mouse at `hit`. A click focuses the pane and selects the
    /// row; the wheel moves the pane under the pointer without focusing it.
    pub fn on_mouse(&mut self, mouse: Mouse, hit: Option<Hit>) -> Option<Effect> {
        if self.show_help {
            self.show_help = mouse != Mouse::Click;
            return None;
        }
        if self.confirm.is_some() || self.compose.is_some() {
            return None;
        }
        let Hit { pane, item } = hit?;
        match mouse {
            Mouse::Click => {
                self.focus = pane;
                let item = item?;
                let selected = match pane {
                    Pane::Folders => self.selected_folder,
                    Pane::Messages => self.selected_message,
                    Pane::Reader => return None,
                };
                self.move_selection(item as isize - selected as isize)
            }
            Mouse::ScrollUp | Mouse::ScrollDown => {
                let focus = std::mem::replace(&mut self.focus, pane);
                let step = match mouse {
                    Mouse::ScrollUp => -WHEEL_STEP,
                    _ => WHEEL_STEP,
                };
                let effect = self.move_selection(step);
                self.focus = focus;
                effect
            }
        }
    }

    pub fn update(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::FocusNextPane => self.focus = self.focus.next(self.folders_hidden),
            Action::FocusPrevPane => self.focus = self.focus.prev(self.folders_hidden),
            Action::ToggleZoom => self.zoomed = !self.zoomed,
            Action::ToggleFolders => {
                self.folders_hidden = !self.folders_hidden;
                if self.folders_hidden && self.focus == Pane::Folders {
                    self.focus = Pane::Messages;
                }
            }
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
            Action::ToggleMark if self.focus == Pane::Messages => {
                if let Some(id) = self.selected().map(|m| m.id.clone())
                    && !self.marked.remove(&id)
                {
                    self.marked.insert(id);
                }
                return self.move_selection(1);
            }
            Action::MarkAll if self.focus == Pane::Messages => {
                self.marked = self.messages.iter().map(|m| m.id.clone()).collect();
            }
            Action::ClearMarks => self.marked.clear(),
            Action::ToggleMark | Action::MarkAll => {}
            Action::Delete | Action::DeletePermanently if self.focus != Pane::Folders => {
                let ids = self.to_act_on();
                if !ids.is_empty() {
                    let permanent = action == Action::DeletePermanently;
                    self.confirm = Some(Confirm::Delete { ids, permanent });
                }
            }
            Action::Delete | Action::DeletePermanently => {}
            Action::Refresh => return Some(Effect::Sync),
            Action::Help => self.show_help = true,
            Action::Compose => self.compose = Some(Compose::new(ComposeKind::New, None)),
            Action::Reply | Action::ReplyAll | Action::Forward => {
                if let Some(original) = self.selected() {
                    let id = original.id.clone();
                    let kind = match action {
                        Action::Reply => ComposeKind::Reply(id),
                        Action::ReplyAll => ComposeKind::ReplyAll(id),
                        _ => ComposeKind::Forward(id),
                    };
                    self.compose = Some(Compose::new(kind, Some(original)));
                }
            }
        }
        None
    }

    /// Shows a newly loaded folder's messages, selecting the first.
    pub fn set_messages(&mut self, messages: Vec<Message>) -> Option<Effect> {
        self.marked.clear();
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
        self.marked
            .retain(|id| messages.iter().any(|m| &m.id == id));
        match selected.and_then(|id| messages.iter().position(|m| m.id == id)) {
            Some(index) => {
                self.messages = messages;
                self.selected_message = index;
                None
            }
            None => {
                let marked = std::mem::take(&mut self.marked);
                let effect = self.set_messages(messages);
                self.marked = marked;
                effect
            }
        }
    }

    /// Drops messages core deleted. The selection stays on the same row, so
    /// the message after a deleted one is selected next.
    pub fn messages_deleted(&mut self, ids: &[MessageId]) -> Option<Effect> {
        let selected = self.selected().map(|m| m.id.clone());
        let gone = |m: &Message| ids.contains(&m.id);
        let before = self.messages.iter().take(self.selected_message);
        let index = before.filter(|m| !gone(m)).count();
        self.messages.retain(|m| !gone(m));
        self.selected_message = index.min(self.messages.len().saturating_sub(1));
        for id in ids {
            self.marked.remove(id);
        }
        let s = if ids.len() == 1 { "" } else { "s" };
        self.show_info(format!("Deleted {} message{s}", ids.len()));
        if self.selected().map(|m| &m.id) == selected.as_ref() {
            None
        } else {
            self.selection_changed()
        }
    }

    /// The marked messages in list order, or else the selected one.
    pub fn to_act_on(&self) -> Vec<MessageId> {
        if self.marked.is_empty() {
            return self.selected().map(|m| m.id.clone()).into_iter().collect();
        }
        let marked = self.messages.iter().filter(|m| self.marked.contains(&m.id));
        marked.map(|m| m.id.clone()).collect()
    }

    fn confirmed(&mut self, confirm: Confirm) -> Option<Effect> {
        match confirm {
            Confirm::Delete { ids, permanent } => {
                let s = if ids.len() == 1 { "" } else { "s" };
                self.show_info(format!("Deleting {} message{s}…", ids.len()));
                Some(Effect::Delete { ids, permanent })
            }
            Confirm::Discard => {
                self.compose = None;
                None
            }
        }
    }

    /// Closes the message core sent.
    pub fn mail_sent(&mut self) {
        self.compose = None;
        self.show_info("Sent");
    }

    /// Keeps the message open so it can be fixed and sent again.
    pub fn send_failed(&mut self, error: &str) {
        if let Some(compose) = &mut self.compose {
            compose.sending = false;
        }
        self.show_error(format!("could not send: {error}"));
    }

    pub fn show_error(&mut self, message: String) {
        self.status = Some(Status::Error(message));
    }

    pub fn show_info(&mut self, message: impl Into<String>) {
        self.status = Some(Status::Info(message.into()));
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
    /// The next pane in Tab order, skipping the folders while they are hidden.
    fn next(self, folders_hidden: bool) -> Self {
        match self {
            Self::Folders => Self::Messages,
            Self::Messages => Self::Reader,
            Self::Reader if folders_hidden => Self::Messages,
            Self::Reader => Self::Folders,
        }
    }

    fn prev(self, folders_hidden: bool) -> Self {
        match self {
            Self::Folders => Self::Reader,
            Self::Messages if folders_hidden => Self::Reader,
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
            ..Default::default()
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

    fn key(app: &mut AppState, code: KeyCode) -> Option<Effect> {
        app.on_key(KeyEvent::from(code), &mut Keymap::default())
    }

    fn message_ids(ids: &[&str]) -> Vec<MessageId> {
        ids.iter().map(|id| MessageId((*id).to_owned())).collect()
    }

    #[test]
    fn d_asks_and_y_moves_the_selected_message_to_deleted_items() {
        let mut app = in_message_list(&["m0", "m1"]);

        assert_eq!(key(&mut app, KeyCode::Char('d')), None);
        assert!(app.confirm.is_some());

        let effect = key(&mut app, KeyCode::Char('y'));
        assert_eq!(
            effect,
            Some(Effect::Delete {
                ids: message_ids(&["m0"]),
                permanent: false
            })
        );
        assert_eq!(app.confirm, None);
    }

    #[test]
    fn shift_d_deletes_the_marked_messages_for_good_after_enter() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.update(Action::Bottom);
        app.update(Action::ToggleMark);
        app.update(Action::Top);
        app.update(Action::ToggleMark);

        key(&mut app, KeyCode::Char('D'));
        let effect = key(&mut app, KeyCode::Enter);

        assert_eq!(
            effect,
            Some(Effect::Delete {
                ids: message_ids(&["m0", "m2"]),
                permanent: true
            }),
            "in list order"
        );
    }

    #[test]
    fn any_other_key_cancels_the_delete_and_does_nothing_else() {
        let mut app = in_message_list(&["m0", "m1"]);
        key(&mut app, KeyCode::Char('d'));

        assert_eq!(key(&mut app, KeyCode::Char('j')), None);

        assert_eq!(app.confirm, None);
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn there_is_nothing_to_delete_in_the_folder_pane_or_an_empty_folder() {
        let mut app = in_message_list(&["m0"]);
        app.focus = Pane::Folders;
        app.update(Action::Delete);
        assert_eq!(app.confirm, None);

        let mut app = in_message_list(&[]);
        app.update(Action::Delete);
        assert_eq!(app.confirm, None);
    }

    #[test]
    fn deleted_messages_leave_the_selection_on_the_next_one() {
        let mut app = in_message_list(&["m0", "m1", "m2", "m3"]);
        app.update(Action::SelectNext);
        app.update(Action::SelectNext);

        let effect = app.messages_deleted(&message_ids(&["m0", "m2"]));

        assert_eq!(effect, load_body("m3"));
        assert_eq!(app.selected().map(|m| m.id.0.as_str()), Some("m3"));
        assert_eq!(
            app.status,
            Some(Status::Info("Deleted 2 messages".to_owned()))
        );
    }

    #[test]
    fn deleting_other_messages_keeps_the_selected_one() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.update(Action::Bottom);
        app.update(Action::MarkAll);

        let effect = app.messages_deleted(&message_ids(&["m0"]));

        assert_eq!(effect, None);
        assert_eq!(app.selected_message, 1);
        assert_eq!(ids(&app.marked), ["m1", "m2"]);
    }

    #[test]
    fn deleting_the_last_message_selects_the_one_before() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.update(Action::Bottom);

        assert_eq!(app.messages_deleted(&message_ids(&["m1"])), load_body("m0"));
    }

    #[test]
    fn question_mark_shows_help_until_the_next_key() {
        let mut app = in_message_list(&["m0", "m1"]);

        key(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);

        assert_eq!(key(&mut app, KeyCode::Char('j')), None);
        assert!(!app.show_help);
        assert_eq!(app.selected_message, 0, "the key only closed the help");
    }

    fn ctrl_s(app: &mut AppState) -> Option<Effect> {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        app.on_key(key, &mut Keymap::default())
    }

    #[test]
    fn r_replies_to_the_selected_message_and_keys_go_to_the_message() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.messages[1].from_addr = "grace@example.com".to_owned();
        app.update(Action::SelectNext);

        key(&mut app, KeyCode::Char('r'));
        for c in "Thanks".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let effect = ctrl_s(&mut app);

        assert_eq!(
            effect,
            Some(Effect::Send(Draft {
                kind: ComposeKind::Reply(MessageId("m1".to_owned())),
                to: vec!["grace@example.com".to_owned()],
                cc: vec![],
                subject: "RE: m1".to_owned(),
                body: "Thanks".to_owned(),
            }))
        );
        assert_eq!(app.selected_message, 1, "j and k were typed, not moved");
        assert_eq!(app.status, Some(Status::Info("Sending…".to_owned())));
    }

    #[test]
    fn replying_needs_a_selected_message() {
        let mut app = in_message_list(&[]);

        app.update(Action::Reply);
        app.update(Action::Forward);

        assert!(app.compose.is_none());
    }

    #[test]
    fn sending_without_anyone_to_send_to_shows_why_and_sends_nothing() {
        let mut app = AppState::default();
        app.update(Action::Compose);

        assert_eq!(ctrl_s(&mut app), None);

        assert_eq!(
            app.status,
            Some(Status::Error("Add someone to send it to".to_owned()))
        );
        assert!(app.compose.is_some());
    }

    #[test]
    fn a_sent_message_closes_and_a_failed_one_stays_to_try_again() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Reply);
        ctrl_s(&mut app);

        app.send_failed("offline");
        assert_eq!(
            app.status,
            Some(Status::Error("could not send: offline".to_owned()))
        );
        assert!(app.compose.as_ref().is_some_and(|c| !c.sending));

        ctrl_s(&mut app);
        app.mail_sent();
        assert!(app.compose.is_none());
        assert_eq!(app.status, Some(Status::Info("Sent".to_owned())));
    }

    #[test]
    fn closing_a_written_message_asks_first() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Reply);
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Esc);

        key(&mut app, KeyCode::Esc);
        assert_eq!(app.confirm, Some(Confirm::Discard));
        key(&mut app, KeyCode::Char('n'));
        assert!(app.compose.is_some(), "any other key keeps writing");

        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char('y'));
        assert!(app.compose.is_none());
    }

    fn hit(pane: Pane, item: Option<usize>) -> Option<Hit> {
        Some(Hit { pane, item })
    }

    #[test]
    fn a_click_focuses_the_pane_and_selects_the_row() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.focus = Pane::Folders;

        let effect = app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(2)));

        assert_eq!(app.focus, Pane::Messages);
        assert_eq!(effect, load_body("m2"));
        assert_eq!(app.on_mouse(Mouse::Click, hit(Pane::Reader, None)), None);
        assert_eq!(app.focus, Pane::Reader);
    }

    #[test]
    fn the_wheel_moves_the_pane_under_the_pointer_without_focusing_it() {
        let mut app = in_message_list(&["m0", "m1", "m2", "m3", "m4"]);
        app.focus = Pane::Reader;

        let effect = app.on_mouse(Mouse::ScrollDown, hit(Pane::Messages, None));

        assert_eq!(effect, load_body("m3"));
        assert_eq!(app.focus, Pane::Reader);
        app.on_mouse(Mouse::ScrollUp, hit(Pane::Messages, Some(0)));
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn a_click_only_closes_the_help() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.show_help = true;

        assert_eq!(
            app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(1))),
            None
        );

        assert!(!app.show_help);
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn the_mouse_waits_while_a_dialog_or_message_is_open() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.update(Action::Delete);

        assert_eq!(
            app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(1))),
            None
        );
        assert_eq!(app.selected_message, 0);
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

    fn ids(marked: &BTreeSet<MessageId>) -> Vec<&str> {
        marked.iter().map(|id| id.0.as_str()).collect()
    }

    #[test]
    fn space_marks_the_message_and_moves_down() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);

        assert_eq!(app.update(Action::ToggleMark), load_body("m1"));
        app.update(Action::ToggleMark);
        assert_eq!(ids(&app.marked), ["m0", "m1"]);

        app.update(Action::SelectPrev);
        app.update(Action::ToggleMark);
        assert_eq!(ids(&app.marked), ["m0"], "a second Space unmarks");
    }

    #[test]
    fn ctrl_a_marks_every_message_and_esc_clears_them() {
        let mut app = in_message_list(&["m0", "m1"]);

        app.update(Action::MarkAll);
        assert_eq!(ids(&app.marked), ["m0", "m1"]);
        app.update(Action::ClearMarks);
        assert!(app.marked.is_empty());
    }

    #[test]
    fn marks_go_with_the_folder_but_survive_a_sync() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.update(Action::MarkAll);

        app.refresh_messages(vec![message("m1"), message("m2")]);
        assert_eq!(ids(&app.marked), ["m1", "m2"]);

        app.set_messages(vec![message("x")]);
        assert!(app.marked.is_empty());
    }

    #[test]
    fn marking_only_works_in_the_message_list() {
        let mut app = in_message_list(&["m0"]);
        app.focus = Pane::Folders;

        app.update(Action::ToggleMark);
        app.update(Action::MarkAll);

        assert!(app.marked.is_empty());
    }

    #[test]
    fn zoom_follows_the_focus_until_toggled_off() {
        let mut app = in_message_list(&["m1"]);

        app.update(Action::ToggleZoom);
        assert_eq!(app.zoomed_pane(), Some(Pane::Messages));
        app.update(Action::FocusNextPane);
        assert_eq!(app.zoomed_pane(), Some(Pane::Reader));
        app.update(Action::ToggleZoom);
        assert_eq!(app.zoomed_pane(), None);
    }

    #[test]
    fn hiding_the_folders_moves_focus_off_them_and_tab_skips_them() {
        let mut app = AppState::default();

        app.update(Action::ToggleFolders);
        assert_eq!(app.focus, Pane::Messages);
        app.update(Action::FocusNextPane);
        app.update(Action::FocusNextPane);
        assert_eq!(app.focus, Pane::Messages);
        app.update(Action::FocusPrevPane);
        assert_eq!(app.focus, Pane::Reader);

        app.update(Action::ToggleFolders);
        app.update(Action::FocusNextPane);
        assert_eq!(app.focus, Pane::Folders, "shown again, Tab reaches them");
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
