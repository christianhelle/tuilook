use std::{
    collections::{BTreeSet, HashSet},
    time::{Duration, Instant},
};

use crate::model::{
    Action, ComposeKind, Draft, Folder, FolderId, Message, MessageId, sort_messages,
};
use jiff::{Timestamp, Zoned};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    widgets::{Paragraph, Wrap},
};

use crate::tui::{
    compose::{Compose, Request},
    dialog::Dialog,
    folder_edit,
    groups::{self, DateGroup, MessageRow},
    keymap::Keymap,
    mover,
    rows::{account_row, is_own_row, row_target, unread_row},
    search::Search,
    signature::{self, SignatureEditor},
    ui::shown,
    viewer::{self, Viewer},
    vim::Register,
};

/// IO the event loop must perform after an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    LoadFolder(FolderId),
    /// Show only the unread messages of a folder.
    LoadUnread(FolderId),
    LoadBody(MessageId),
    /// Fetch a page of a folder's mail from before what sync reaches.
    LoadOlder {
        folder: FolderId,
        before: Timestamp,
        unread_only: bool,
    },
    /// Mark a message read, as the reading pane shows it.
    MarkRead(MessageId),
    /// Pin messages to the top of their folders, or unpin them.
    SetPinned {
        ids: Vec<MessageId>,
        pinned: bool,
    },
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
    },
    /// Delete a folder: to Deleted Items, or for good when `permanent`.
    DeleteFolder {
        folder: FolderId,
        permanent: bool,
    },
    /// Make a folder in `account`, inside `parent` or at the top level.
    CreateFolder {
        account: usize,
        parent: Option<FolderId>,
        name: String,
    },
    /// Give a folder of `account` a new name.
    RenameFolder {
        account: usize,
        folder: FolderId,
        name: String,
    },
    /// Move messages of `account` into folder `to`.
    Move {
        account: usize,
        ids: Vec<MessageId>,
        to: FolderId,
    },
    /// Sync every folder.
    Sync,
    Send(Draft),
    /// Search every account's cached mail.
    Search(String),
    /// Keep `text` as the account's signature.
    SaveSignature {
        account: usize,
        text: String,
    },
    /// Put text on the clipboard.
    Copy(String),
}

/// A question the user must answer before anything else happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
    },
    /// Delete a folder and everything in it.
    DeleteFolder {
        folder: FolderId,
        name: String,
        permanent: bool,
    },
    /// Close the message being written, losing what is in it.
    Discard,
    /// Send although several messages went out just before it.
    SendAnyway { draft: Draft, sent_lately: usize },
    /// Close the signature being written, losing the changes.
    DiscardSignature,
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

/// A line for the status bar, shown instead of the key hints for a while.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
    /// Work in progress, shown until its outcome replaces it.
    Busy(String),
}

impl Status {
    /// How long it stays before the key hints come back, if it goes by itself.
    fn lasts(&self) -> Option<Duration> {
        match self {
            Status::Info(_) => Some(Duration::from_secs(4)),
            // Errors stay longer, to be read.
            Status::Error(_) => Some(Duration::from_secs(8)),
            Status::Busy(_) => None,
        }
    }
}

/// How long after sending to sync again, to show the sent message.
const AFTER_SENDING: Duration = Duration::from_secs(5);

/// Sending more than this many messages within [`PACE_WINDOW`] asks first:
/// Outlook.com may block an account that sends in bursts.
pub const PACE_LIMIT: usize = 3;
pub const PACE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// When to sync with the server next.
#[derive(Debug, Default)]
pub struct SyncState {
    /// Whether core is syncing now.
    pub running: bool,
    /// Accounts whose core has yet to finish the running sync.
    pub pending: usize,
    /// When the next sync is due, if one is.
    pub next: Option<Instant>,
    /// How often to sync on its own; never when `None`.
    pub every: Option<Duration>,
    /// Whether the running sync was asked for with F5, which also lets the
    /// unread list drop the mail read in it.
    pub forced: bool,
}

impl SyncState {
    /// Makes a sync due no later than `at`.
    fn due_by(&mut self, at: Instant) {
        self.next = Some(self.next.map_or(at, |next| next.min(at)));
    }
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
    /// Columns the reading pane wraps its lines to.
    pub reader_width: u16,
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
    /// The date group whose header is selected, instead of a message. The
    /// selected message is then the group's first.
    pub selected_group: Option<DateGroup>,
    /// Date groups whose messages are folded away, in every folder.
    pub collapsed_groups: BTreeSet<DateGroup>,
    /// The selected message's body, once core has loaded it.
    pub body: Option<String>,
    pub status: Option<Status>,
    /// When the status was shown, to know when it has gone stale.
    pub status_since: Option<Instant>,
    pub sync: SyncState,
    pub viewport: Viewport,
    /// Lines of the reading pane scrolled out of view at the top.
    pub reader_scroll: usize,
    /// Whether the focused pane fills the screen.
    pub zoomed: bool,
    pub folders_hidden: bool,
    /// Messages picked for acting on together, such as deleting.
    pub marked: BTreeSet<MessageId>,
    /// Folders whose subfolders are shown.
    pub expanded: BTreeSet<FolderId>,
    /// The dialog over the panes, which takes every key.
    pub dialog: Option<Dialog>,
    /// Which account sent recent messages, and when, to notice a burst.
    pub sent: Vec<(usize, Instant)>,
    /// A message being written, which takes over the screen.
    pub compose: Option<Compose>,
    /// The signed-in addresses. With more than one, each account heads its
    /// own folders in the folder pane.
    pub accounts: Vec<String>,
    /// Each account's signature, put below each new message it writes.
    pub signatures: Vec<String>,
    /// An account's signature being written, which takes over the screen.
    pub signature: Option<SignatureEditor>,
    /// Each account's folders as core last reported them, or `None` before
    /// the first report.
    pub account_folders: Vec<Option<Vec<Folder>>>,
    /// The account each of `folders` belongs to.
    pub folder_account: Vec<usize>,
    /// Folders shown again in the favorites above the accounts, in the
    /// order they were added.
    pub favorites: Vec<FolderId>,
    /// The reading pane's cursor, which picks text to copy.
    pub viewer: Viewer,
    /// The text last deleted or yanked, to put with `p`.
    pub register: Register,
    /// A search being typed, or whose results the message list shows.
    pub search: Option<Search>,
    /// The folder whose older mail is being fetched, and whether only its
    /// unread mail.
    pub loading_older: Option<(FolderId, bool)>,
    /// Folders with no older mail left to fetch, all of it or only unread.
    /// Forgotten after each sync, as new counts may say otherwise.
    pub no_older: HashSet<(FolderId, bool)>,
    /// When it is, read once a frame so a key and the drawing after it put
    /// mail in the same date groups. Unset, it is read whenever asked.
    pub clock: Option<Zoned>,
}

impl AppState {
    /// The lines of the reading pane, before wrapping: the selected
    /// message's headers and body.
    pub fn reader_text(&self) -> Option<Vec<String>> {
        let m = self.messages.get(self.selected_message)?;
        let mut text = vec![
            format!("From: {} <{}>", shown(&m.from_name), shown(&m.from_addr)),
            format!("Subject: {}", shown(&m.subject)),
            String::new(),
        ];
        // The preview stands in until core has loaded the body.
        let body = self.body.as_deref().unwrap_or(&m.preview);
        text.extend(shown(body).lines().map(str::to_owned));
        Some(text)
    }

    /// The rows each line of the reading pane takes once wrapped.
    fn reader_rows(&self) -> Vec<usize> {
        self.reader_rows_at(self.viewport.reader_width)
    }

    /// The rows each line of the reading pane takes once wrapped to `width`
    /// columns. Scrolling, the cursor and the drawing all count with this.
    pub fn reader_rows_at(&self, width: u16) -> Vec<usize> {
        let width = width.max(1);
        self.reader_text()
            .unwrap_or_default()
            .into_iter()
            .map(|line| {
                Paragraph::new(line)
                    .wrap(Wrap { trim: false })
                    .line_count(width)
            })
            .collect()
    }

    /// Scrolls the reading pane as little as it takes to show all of `line`.
    fn show_reader_line(&mut self, line: usize) {
        let rows = self.reader_rows();
        let top: usize = rows.iter().take(line).sum();
        let bottom = top + rows.get(line).copied().unwrap_or(1);
        // A line taller than the pane shows from its top.
        let lowest = bottom.saturating_sub(self.viewport.reader).min(top);
        self.reader_scroll = self.reader_scroll.max(lowest).min(top);
    }

    /// Moves the reader's cursor onto the lines shown, as Vim does when
    /// the view scrolls away from it.
    fn keep_cursor_in_view(&mut self) {
        let Some(text) = self.reader_text() else {
            return;
        };
        self.viewer.show(&text);
        let top = self.reader_line_at(self.reader_scroll);
        let last_row = self.reader_scroll + self.viewport.reader.saturating_sub(1);
        let bottom = self.reader_line_at(last_row).max(top);
        let (line, column) = self.viewer.cursor();
        self.viewer.move_to(line.clamp(top, bottom), column);
    }

    /// The line of the reading pane shown on wrapped row `row`, or the
    /// last line past the end.
    fn reader_line_at(&self, row: usize) -> usize {
        let rows = self.reader_rows();
        let mut top = 0;
        for (line, height) in rows.iter().enumerate() {
            top += height;
            if row < top {
                return line;
            }
        }
        rows.len().saturating_sub(1)
    }

    /// The question asked, if one is.
    pub fn confirm(&self) -> Option<&Confirm> {
        match &self.dialog {
            Some(Dialog::Confirm(confirm)) => Some(confirm),
            _ => None,
        }
    }

    /// What time it is, by the clock read this frame.
    pub fn now(&self) -> Zoned {
        self.clock.clone().unwrap_or_else(Zoned::now)
    }

    /// The pane filling the screen, which follows the focus while zoomed.
    pub fn zoomed_pane(&self) -> Option<Pane> {
        self.zoomed.then_some(self.focus)
    }

    /// Handles a key press: an open dialog takes it first, then the keymap.
    pub fn on_key(&mut self, key: KeyEvent, keymap: &mut Keymap) -> Option<Effect> {
        let key = alt_gr_as_text(key);
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return None;
        }
        if let Some(dialog) = self.dialog.take() {
            return self.dialog_key(dialog, key);
        }
        if let Some(compose) = &mut self.compose {
            match compose.key(key, self.viewport.compose, &mut self.register) {
                Request::None => {}
                Request::Send(draft) => {
                    let account = compose.account;
                    let sent_lately = self.sent_lately(account, Instant::now());
                    if sent_lately >= PACE_LIMIT {
                        self.dialog =
                            Some(Dialog::Confirm(Confirm::SendAnyway { draft, sent_lately }));
                        return None;
                    }
                    self.show_busy("Sending…");
                    return Some(Effect::Send(draft));
                }
                Request::Close => self.compose = None,
                Request::ConfirmDiscard => self.dialog = Some(Dialog::Confirm(Confirm::Discard)),
                Request::Help => self.dialog = Some(Dialog::Help),
                Request::Invalid(why) => self.show_error(why),
            }
            return None;
        }
        if let Some(editor) = &mut self.signature {
            match editor.key(key, self.viewport.compose, &mut self.register) {
                signature::Request::None => {}
                signature::Request::Save(text) => {
                    let account = editor.account;
                    if self.signatures.len() <= account {
                        self.signatures.resize(account + 1, String::new());
                    }
                    self.signatures[account] = text.clone();
                    self.signature = None;
                    self.show_info("Signature saved");
                    return Some(Effect::SaveSignature { account, text });
                }
                signature::Request::Close => self.signature = None,
                signature::Request::ConfirmDiscard => {
                    self.dialog = Some(Dialog::Confirm(Confirm::DiscardSignature));
                }
                signature::Request::Help => self.dialog = Some(Dialog::Help),
            }
            return None;
        }
        if self.typing_search() {
            return self.search_key(key);
        }
        if self.focus == Pane::Reader
            && let Some(text) = self.reader_text()
        {
            self.viewer.show(&text);
            match self
                .viewer
                .key(key, self.viewport.reader, &mut self.register)
            {
                viewer::Request::None => {
                    let (line, _) = self.viewer.cursor();
                    self.show_reader_line(line);
                    return None;
                }
                viewer::Request::Copy(text) => {
                    match text.lines().count() {
                        1 => self.show_info("Copied 1 line"),
                        lines => self.show_info(format!("Copied {lines} lines")),
                    }
                    return Some(Effect::Copy(text));
                }
                viewer::Request::Unhandled => {}
            }
        }
        keymap
            .action_for(key)
            .and_then(|action| self.update(action))
    }

    /// A key for the open `dialog`, taken out of the state: it goes back
    /// unless the key closed it.
    fn dialog_key(&mut self, dialog: Dialog, key: KeyEvent) -> Option<Effect> {
        match dialog {
            Dialog::Help | Dialog::Blocked(_) => None,
            Dialog::Confirm(confirm) => {
                // y or Enter agrees; any other key cancels and does nothing else.
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Enter) {
                    return self.confirmed(confirm);
                }
                self.cancelled(confirm);
                None
            }
            Dialog::Name(mut prompt) => {
                let request = prompt.key(key);
                if request == folder_edit::Request::None {
                    self.dialog = Some(Dialog::Name(prompt));
                }
                self.name_requested(request)
            }
            Dialog::Move(mut mover) => {
                let request = mover.key(key);
                if request == mover::Request::None {
                    self.dialog = Some(Dialog::Move(mover));
                }
                self.move_requested(request)
            }
        }
    }

    /// Handles the mouse at `hit`. A click focuses the pane and selects the
    /// row; the wheel moves the pane under the pointer without focusing it.
    pub fn on_mouse(&mut self, mouse: Mouse, hit: Option<Hit>) -> Option<Effect> {
        if let Some(dialog) = &self.dialog {
            // A click closes the help or a blocked send; the other dialogs
            // want an answer from the keyboard.
            if mouse == Mouse::Click && matches!(dialog, Dialog::Help | Dialog::Blocked(_)) {
                self.dialog = None;
            }
            return None;
        }
        if self.compose.is_some() || self.signature.is_some() {
            return None;
        }
        let Hit { pane, item } = hit?;
        match mouse {
            Mouse::Click => {
                self.focus = pane;
                let item = item?;
                match pane {
                    Pane::Folders => self.select_folder(item),
                    Pane::Messages => self.click_row(item),
                    Pane::Reader => None,
                }
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
            Action::Open if self.focus == Pane::Messages && self.selected_group.is_some() => {
                return self.toggle_group();
            }
            Action::Open => self.open(),
            Action::Expand if self.focus == Pane::Folders => return self.expand_folder(),
            Action::Collapse if self.focus == Pane::Folders => return self.collapse_folder(),
            Action::Expand if self.focus == Pane::Messages => return self.expand_group(),
            Action::Collapse if self.focus == Pane::Messages => return self.collapse_group(),
            Action::Expand if self.focus == Pane::Reader => return self.step_message(1),
            Action::Collapse if self.focus == Pane::Reader => return self.step_message(-1),
            Action::Expand | Action::Collapse => {}
            Action::ToggleFavorite if self.focus == Pane::Folders => {
                return self.toggle_favorite();
            }
            Action::ToggleFavorite => {}
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
            Action::ClearMarks if self.marked.is_empty() => return self.leave_search(),
            Action::ClearMarks => self.marked.clear(),
            Action::ToggleMark | Action::MarkAll => {}
            Action::TogglePin if self.focus != Pane::Folders => return self.toggle_pin(),
            Action::TogglePin => {}
            Action::Delete | Action::DeletePermanently if self.focus != Pane::Folders => {
                let ids = self.to_act_on();
                if !ids.is_empty() {
                    let permanent = action == Action::DeletePermanently;
                    self.dialog = Some(Dialog::Confirm(Confirm::Delete { ids, permanent }));
                }
            }
            Action::Delete | Action::DeletePermanently => {
                self.ask_to_delete_folder(action == Action::DeletePermanently);
            }
            Action::AddFolder | Action::AddTopFolder if self.focus == Pane::Folders => {
                self.ask_folder_name(action == Action::AddTopFolder);
            }
            Action::RenameFolder if self.focus == Pane::Folders => self.ask_new_name(),
            Action::AddFolder | Action::AddTopFolder | Action::RenameFolder => {}
            Action::Move => self.ask_where_to_move(),
            Action::Refresh if self.sync.running => {
                self.sync.forced = true;
                self.show_busy("Syncing…");
            }
            Action::Refresh => {
                self.show_busy("Syncing…");
                let effect = self.start_sync();
                self.sync.forced = true;
                return effect;
            }
            Action::Help => self.dialog = Some(Dialog::Help),
            Action::Search => self.start_search(),
            Action::Compose => self.start_compose(Compose::new(ComposeKind::New, None)),
            Action::EditSignature => {
                let account = self.current_account();
                let signature = self.signatures.get(account).map_or("", String::as_str);
                self.signature = Some(SignatureEditor::new(account, signature));
            }
            Action::Reply | Action::ReplyAll | Action::Forward => {
                if let Some(original) = self.selected() {
                    let id = original.id.clone();
                    let kind = match action {
                        Action::Reply => ComposeKind::Reply(id),
                        Action::ReplyAll => ComposeKind::ReplyAll(id),
                        _ => ComposeKind::Forward(id),
                    };
                    let compose = Compose::new(kind, Some(original));
                    self.start_compose(compose);
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
        self.settle_selection();
        self.selection_changed()
    }

    /// Shows the only account's newly synced folder tree.
    pub fn set_folders(&mut self, folders: Vec<Folder>) -> Option<Effect> {
        self.set_account_folders(0, folders)
    }

    /// Shows an account's newly synced folder tree, keeping the selected
    /// folder.
    pub fn set_account_folders(&mut self, account: usize, folders: Vec<Folder>) -> Option<Effect> {
        if self.account_folders.len() <= account {
            self.account_folders.resize(account + 1, None);
        }
        let first = self.account_folders[account].replace(folders).is_none();
        if first && let Some(address) = self.accounts.get(account) {
            // An account starts with its folders shown, in both sections.
            self.expanded.insert(account_row(address));
            self.expanded.insert(unread_row(&account_row(address)));
        }
        self.rebuild_folders()
    }

    /// Lays out the folder pane again from the accounts' folders, the
    /// unread section and the favorites, keeping the selected row, or
    /// failing that the folder it showed.
    pub(crate) fn rebuild_folders(&mut self) -> Option<Effect> {
        let previous = self.current_folder().cloned();
        let shown = self.load_shown();
        let (folders, owners) = self.folder_tree();
        let find = |id: &FolderId| folders.iter().position(|f| &f.id == id);
        self.selected_folder = previous
            .as_ref()
            .and_then(|id| find(id).or_else(|| find(&row_target(id))))
            // Otherwise the first real folder, so there is mail to look at.
            .or_else(|| folders.iter().position(is_own_row))
            .unwrap_or(0);
        self.folders = folders;
        self.folder_account = owners;
        self.reveal_selected_folder();
        let current = self.load_shown();
        // Search results stay until the search is left.
        (current != shown && self.results_query().is_none())
            .then_some(current)
            .flatten()
    }

    /// Loads what the selected row shows: its folder's messages, or only
    /// the unread ones for a row in the unread section.
    pub(crate) fn load_shown(&self) -> Option<Effect> {
        let id = self.shown_folder()?;
        Some(if self.shows_unread() {
            Effect::LoadUnread(id)
        } else {
            Effect::LoadFolder(id)
        })
    }

    /// The id of the selected row, which for a favorite or unread row is not
    /// the folder's.
    pub fn current_folder(&self) -> Option<&FolderId> {
        self.folders.get(self.selected_folder).map(|f| &f.id)
    }

    /// The folder whose messages are shown.
    pub fn shown_folder(&self) -> Option<FolderId> {
        self.current_folder().map(row_target)
    }

    /// The account of the selected folder, and so of the messages shown.
    pub fn current_account(&self) -> usize {
        self.account_of(self.selected_folder)
    }

    /// The account the folder at `index` belongs to.
    pub fn account_of(&self, index: usize) -> usize {
        self.folder_account.get(index).copied().unwrap_or(0)
    }

    /// The account whose core must perform `effect`. A sync goes to every
    /// account instead.
    pub fn account_for(&self, effect: &Effect) -> usize {
        match effect {
            Effect::LoadFolder(id)
            | Effect::LoadUnread(id)
            | Effect::LoadOlder { folder: id, .. }
            | Effect::DeleteFolder { folder: id, .. } => self
                .folders
                .iter()
                .position(|f| &f.id == id)
                .map_or(0, |index| self.account_of(index)),
            Effect::LoadBody(id) | Effect::MarkRead(id) => self.message_account(id),
            Effect::Delete { .. }
            | Effect::SetPinned { .. }
            | Effect::Sync
            | Effect::Search(_)
            | Effect::Copy(_) => self.current_account(),
            Effect::Send(_) => self.compose.as_ref().map_or(0, |c| c.account),
            Effect::SaveSignature { account, .. }
            | Effect::CreateFolder { account, .. }
            | Effect::RenameFolder { account, .. }
            | Effect::Move { account, .. } => *account,
        }
    }

    /// Every account's folders as one tree, with the account each is in.
    /// With several accounts, each heads a row of its own above its folders.
    /// The favorites come first, then the unread section.
    fn folder_tree(&self) -> (Vec<Folder>, Vec<usize>) {
        let several = self.accounts.len() > 1;
        let (mut tree, mut owners) = self.favorite_rows();
        let (unread, unread_owners) = self.unread_rows();
        tree.extend(unread);
        owners.extend(unread_owners);
        for (account, folders) in self.account_folders.iter().enumerate() {
            let folders = folders.as_deref().unwrap_or_default();
            let Some(address) = self.accounts.get(account).filter(|_| several) else {
                tree.extend_from_slice(folders);
                owners.resize(tree.len(), account);
                continue;
            };
            let row = account_row(address);
            tree.push(Folder {
                id: row.clone(),
                parent_id: None,
                name: address.clone(),
                well_known: None,
                unread: 0,
                total: 0,
            });
            // The account's top folders are those whose parent it does not list.
            let ids: HashSet<&FolderId> = folders.iter().map(|f| &f.id).collect();
            tree.extend(folders.iter().map(|folder| {
                let mut folder = folder.clone();
                if !folder.parent_id.as_ref().is_some_and(|p| ids.contains(p)) {
                    folder.parent_id = Some(row.clone());
                }
                folder
            }));
            owners.resize(tree.len(), account);
        }
        (tree, owners)
    }

    /// Opens a message to write from the account being looked at, or for
    /// a reply or forward, the account the original is in.
    fn start_compose(&mut self, compose: Compose) {
        let account = match (&compose.kind, self.selected()) {
            (ComposeKind::New, _) | (_, None) => self.current_account(),
            (_, Some(original)) => self.message_account(&original.id),
        };
        let signature = self.signatures.get(account).map_or("", String::as_str);
        let mut compose = compose.with_signature(signature);
        compose.account = account;
        if self.accounts.len() > 1 {
            compose.from = self.accounts.get(compose.account).cloned();
        }
        self.compose = Some(compose);
    }

    /// Shows the current folder's messages after a sync, keeping the
    /// selected message when it is still there.
    pub fn refresh_messages(&mut self, messages: Vec<Message>) -> Option<Effect> {
        let header = self.selected_group;
        let selected = self.selected().map(|m| m.id.clone());
        self.marked
            .retain(|id| messages.iter().any(|m| &m.id == id));
        if let Some(group) = header {
            // A selected header stays selected while its group has mail.
            let now = self.now();
            let first =
                (0..messages.len()).find(|&i| groups::group_at(&messages, i, &now) == Some(group));
            if let Some(index) = first {
                self.messages = messages;
                self.selected_message = index;
                return None;
            }
        }
        match selected.and_then(|id| messages.iter().position(|m| m.id == id)) {
            Some(index) => {
                self.messages = messages;
                self.selected_message = index;
                self.settle_selection();
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

    /// Drops messages core deleted.
    pub fn messages_deleted(&mut self, ids: &[MessageId]) -> Option<Effect> {
        let s = if ids.len() == 1 { "" } else { "s" };
        self.show_info(format!("Deleted {} message{s}", ids.len()));
        self.drop_messages(ids)
    }

    /// Drops messages core moved to folder `to`.
    pub fn messages_moved(&mut self, ids: &[MessageId], to: &FolderId) -> Option<Effect> {
        let s = if ids.len() == 1 { "" } else { "s" };
        let name = self
            .folders
            .iter()
            .find(|f| &f.id == to)
            .map_or("another folder", |f| f.name.as_str());
        self.show_info(format!("Moved {} message{s} to {name}", ids.len()));
        self.drop_messages(ids)
    }

    /// Drops messages that left the list. The selection stays on the same
    /// row, so the message after a dropped one is selected next.
    fn drop_messages(&mut self, ids: &[MessageId]) -> Option<Effect> {
        let selected = self.selected().map(|m| m.id.clone());
        let gone = |m: &Message| ids.contains(&m.id);
        let before = self.messages.iter().take(self.selected_message);
        let index = before.filter(|m| !gone(m)).count();
        self.messages.retain(|m| !gone(m));
        self.selected_message = index.min(self.messages.len().saturating_sub(1));
        self.settle_selection();
        for id in ids {
            self.marked.remove(id);
        }
        if self.selected().map(|m| &m.id) == selected.as_ref() {
            None
        } else {
            self.selection_changed()
        }
    }

    /// Pins the marked or selected messages, or unpins them when they all
    /// are, moving them in the list while the cursor stays on its message.
    fn toggle_pin(&mut self) -> Option<Effect> {
        let ids = self.to_act_on();
        if ids.is_empty() {
            return None;
        }
        let acting = |m: &Message| ids.contains(&m.id);
        let pinned = !self.messages.iter().filter(|m| acting(m)).all(|m| m.pinned);
        let selected = self.selected().map(|m| m.id.clone());
        for m in self.messages.iter_mut().filter(|m| ids.contains(&m.id)) {
            m.pinned = pinned;
        }
        sort_messages(&mut self.messages);
        if let Some(index) = selected.and_then(|id| self.messages.iter().position(|m| m.id == id)) {
            self.selected_message = index;
        }
        self.settle_selection();
        Some(Effect::SetPinned { ids, pinned })
    }

    /// The marked messages in list order, or else the selected one.
    pub fn to_act_on(&self) -> Vec<MessageId> {
        if self.marked.is_empty() {
            return self.selected().map(|m| m.id.clone()).into_iter().collect();
        }
        let marked = self.messages.iter().filter(|m| self.marked.contains(&m.id));
        marked.map(|m| m.id.clone()).collect()
    }

    /// Messages `account` sent within [`PACE_WINDOW`] before `now`.
    pub fn sent_lately(&self, account: usize, now: Instant) -> usize {
        let lately = |(from, at): &&(usize, Instant)| {
            *from == account && now.saturating_duration_since(*at) < PACE_WINDOW
        };
        self.sent.iter().filter(lately).count()
    }

    fn cancelled(&mut self, confirm: Confirm) {
        if let Confirm::SendAnyway { .. } = confirm {
            if let Some(compose) = &mut self.compose {
                compose.sending = false;
            }
            self.show_info("Not sent; the message is still open");
        }
    }

    fn confirmed(&mut self, confirm: Confirm) -> Option<Effect> {
        match confirm {
            Confirm::Delete { ids, permanent } => {
                let s = if ids.len() == 1 { "" } else { "s" };
                self.show_busy(format!("Deleting {} message{s}…", ids.len()));
                Some(Effect::Delete { ids, permanent })
            }
            Confirm::DeleteFolder {
                folder,
                name,
                permanent,
            } => {
                self.show_busy(format!("Deleting {name}…"));
                Some(Effect::DeleteFolder { folder, permanent })
            }
            Confirm::Discard => {
                self.compose = None;
                None
            }
            Confirm::DiscardSignature => {
                self.signature = None;
                None
            }
            Confirm::SendAnyway { draft, .. } => {
                self.show_busy("Sending…");
                Some(Effect::Send(draft))
            }
        }
    }

    /// Closes the message core sent, and syncs again shortly: Outlook files
    /// a sent message a few seconds after taking it.
    pub fn mail_sent(&mut self, now: Instant) {
        let account = self.compose.as_ref().map_or(0, |c| c.account);
        self.sent
            .retain(|(_, at)| now.saturating_duration_since(*at) < PACE_WINDOW);
        self.sent.push((account, now));
        self.compose = None;
        self.show_info("Sent");
        self.sync.due_by(now + AFTER_SENDING);
    }

    /// Asks core to sync every folder, unless it already is.
    pub fn start_sync(&mut self) -> Option<Effect> {
        if self.sync.running {
            return None;
        }
        self.sync.running = true;
        self.sync.pending = self.accounts.len().max(1);
        self.sync.next = None;
        Some(Effect::Sync)
    }

    /// Starts a sync when the next one is due and none is running.
    pub fn sync_if_due(&mut self, now: Instant) -> Option<Effect> {
        let due = self.sync.next.is_some_and(|next| now >= next);
        if due { self.start_sync() } else { None }
    }

    /// An account's core finished syncing. Once every account has, the next
    /// sync is due one interval from now.
    pub fn sync_finished(&mut self, now: Instant) {
        self.sync.pending = self.sync.pending.saturating_sub(1);
        if self.sync.pending > 0 {
            return;
        }
        self.sync.running = false;
        self.sync.forced = false;
        self.no_older.clear();
        if let Some(every) = self.sync.every {
            self.sync.due_by(now + every);
        }
        // An F5 waiting for this gets an answer, unless an error took its place.
        if self.status == Some(Status::Busy("Syncing…".to_owned())) {
            self.show_info("Up to date");
        }
    }

    /// Keeps the message open so it can be fixed and sent again.
    pub fn send_failed(&mut self, error: &str) {
        if let Some(compose) = &mut self.compose {
            compose.sending = false;
        }
        self.show_error(format!("could not send: {error}"));
    }

    /// Explains that the account may not send for now. The message stays
    /// open, unsent, to send once the account is unblocked.
    pub fn send_blocked(&mut self, said: String) {
        if let Some(compose) = &mut self.compose {
            compose.sending = false;
        }
        self.show_error("Outlook.com has blocked sending from this account".to_owned());
        self.dialog = Some(Dialog::Blocked(said));
    }

    pub fn show_error(&mut self, message: String) {
        self.show(Status::Error(message));
    }

    pub fn show_info(&mut self, message: impl Into<String>) {
        self.show(Status::Info(message.into()));
    }

    pub fn show_busy(&mut self, message: impl Into<String>) {
        self.show(Status::Busy(message.into()));
    }

    fn show(&mut self, status: Status) {
        self.status = Some(status);
        self.status_since = Some(Instant::now());
    }

    /// Drops a status that has been shown long enough, so the key hints
    /// come back.
    pub fn expire_status(&mut self, now: Instant) {
        let lasts = self.status.as_ref().and_then(Status::lasts);
        if let (Some(lasts), Some(since)) = (lasts, self.status_since)
            && now.saturating_duration_since(since) >= lasts
        {
            self.status = None;
            self.status_since = None;
        }
    }

    /// Shows a body core loaded, unless the user has moved on, and marks
    /// the message read if it was not.
    pub fn body_loaded(&mut self, message: MessageId, text: String) -> Option<Effect> {
        if self.selected_group.is_some() {
            return None;
        }
        let shown = self.messages.get_mut(self.selected_message)?;
        if shown.id != message {
            return None;
        }
        self.body = Some(text);
        if shown.is_read {
            return None;
        }
        shown.is_read = true;
        Some(Effect::MarkRead(message))
    }

    /// The selected message; none while a date header is selected.
    pub(crate) fn selected(&self) -> Option<&Message> {
        if self.selected_group.is_some() {
            return None;
        }
        self.messages.get(self.selected_message)
    }

    /// The message list's rows as they are now: date headers, and the
    /// messages of the groups not folded away.
    pub fn message_rows(&self) -> Vec<MessageRow> {
        groups::message_rows(&self.messages, &self.collapsed_groups, &self.now())
    }

    /// The row of `rows` the cursor is on: the selected header, or the
    /// selected message, or the header of the group it is folded in.
    pub fn selected_row(&self, rows: &[MessageRow]) -> Option<usize> {
        let group = self
            .selected_group
            .or_else(|| self.group_at(self.selected_message))?;
        rows.iter().position(|row| match *row {
            MessageRow::Message(index) => {
                self.selected_group.is_none() && index == self.selected_message
            }
            MessageRow::Header {
                group: header,
                collapsed,
                ..
            } => header == group && (self.selected_group.is_some() || collapsed),
        })
    }

    /// The date group of the message at `index`.
    pub(crate) fn group_at(&self, index: usize) -> Option<DateGroup> {
        groups::group_at(&self.messages, index, &self.now())
    }

    /// A click on row `index` of the message list selects it, and on a date
    /// header also folds or unfolds its group.
    fn click_row(&mut self, index: usize) -> Option<Effect> {
        let rows = self.message_rows();
        let row = *rows.get(index)?;
        let moved = self.selected_row(&rows) != Some(index);
        self.select_row(row);
        if matches!(row, MessageRow::Header { .. }) {
            self.toggle_group();
        }
        moved.then(|| self.selection_changed()).flatten()
    }

    /// → in the message list: unfolds the selected date group, or if it
    /// already is, moves to its first message.
    fn expand_group(&mut self) -> Option<Effect> {
        let group = self.selected_group?;
        if self.collapsed_groups.remove(&group) {
            return None;
        }
        self.move_selection(1)
    }

    /// ← in the message list: folds the group of the selected message or
    /// header, leaving the cursor on its header.
    fn collapse_group(&mut self) -> Option<Effect> {
        let group = self
            .selected_group
            .or_else(|| self.group_at(self.selected_message))?;
        self.collapsed_groups.insert(group);
        let moved = self.selected_group.replace(group).is_none();
        self.select_row(MessageRow::Header {
            group,
            count: 0,
            collapsed: true,
        });
        moved.then(|| self.selection_changed()).flatten()
    }

    /// → or ← in the reader: shows the next or previous message in the
    /// list, past date headers and the mail of folded groups.
    fn step_message(&mut self, delta: isize) -> Option<Effect> {
        let rows = self.message_rows();
        let at = self.selected_row(&rows)?;
        let is_message = |row: &&MessageRow| matches!(row, MessageRow::Message(_));
        let row = if delta > 0 {
            rows[at + 1..].iter().find(is_message)
        } else {
            rows[..at].iter().rev().find(is_message)
        }?;
        self.select_row(*row);
        self.selection_changed()
    }

    /// Enter or a click on a date header: folds or unfolds its group.
    pub(crate) fn toggle_group(&mut self) -> Option<Effect> {
        let group = self.selected_group?;
        if !self.collapsed_groups.remove(&group) {
            self.collapsed_groups.insert(group);
        }
        None
    }

    /// Puts the cursor on `row`: a header, or a message.
    pub(crate) fn select_row(&mut self, row: MessageRow) {
        match row {
            MessageRow::Header { group, .. } => {
                self.selected_group = Some(group);
                if let Some(first) =
                    (0..self.messages.len()).find(|&i| self.group_at(i) == Some(group))
                {
                    self.selected_message = first;
                }
            }
            MessageRow::Message(index) => {
                self.selected_group = None;
                self.selected_message = index;
            }
        }
    }

    /// Selects the selected message's row, or its group's header when the
    /// group is folded away.
    fn settle_selection(&mut self) {
        self.selected_group = self
            .group_at(self.selected_message)
            .filter(|group| self.collapsed_groups.contains(group));
    }

    /// Drops the old body and asks for the newly selected one.
    fn selection_changed(&mut self) -> Option<Effect> {
        self.body = None;
        self.reader_scroll = 0;
        self.viewer = Viewer::default();
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
                // Move among the folders shown, past those folded away.
                let visible = self.visible_folders();
                let at = visible.iter().position(|&i| i == self.selected_folder)?;
                let mut to = at;
                step(&mut to, visible.len(), delta);
                self.select_folder(visible[to])
            }
            Pane::Messages => {
                // Move among the rows shown: headers, and unfolded messages.
                let rows = self.message_rows();
                let at = self.selected_row(&rows)?;
                let mut to = at;
                if !step(&mut to, rows.len(), delta) {
                    return None;
                }
                self.select_row(rows[to]);
                self.selection_changed()
            }
            Pane::Reader => {
                // Scroll until the last line is at the bottom of the pane.
                let max = self
                    .viewport
                    .reader_lines
                    .saturating_sub(self.viewport.reader);
                self.reader_scroll = self.reader_scroll.saturating_add_signed(delta).min(max);
                self.keep_cursor_in_view();
                None
            }
        }
    }
}

/// Windows reports a character typed with AltGr, such as `@` on a Danish
/// keyboard (AltGr+2), as Ctrl+Alt and the character. Without this it would
/// look like a Ctrl shortcut, and nothing would be typed.
fn alt_gr_as_text(mut key: KeyEvent) -> KeyEvent {
    let alt_gr = KeyModifiers::CONTROL | KeyModifiers::ALT;
    if matches!(key.code, KeyCode::Char(_)) && key.modifiers.contains(alt_gr) {
        key.modifiers.remove(alt_gr);
    }
    key
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
    use crate::tui::vim::VimMode;

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
            pinned: false,
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
    fn date_groups_count_from_the_clock_read_this_frame() {
        let mut app = in_message_list(&["m1"]);

        app.clock = Some(jiff::Timestamp::UNIX_EPOCH.to_zoned(jiff::tz::TimeZone::UTC));

        assert!(matches!(
            app.message_rows().first(),
            Some(MessageRow::Header {
                group: DateGroup::Today,
                ..
            })
        ));
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

    /// Types each character as a key into the app.
    fn type_keys(app: &mut AppState, keys: &str) -> Option<Effect> {
        let mut keymap = Keymap::default();
        keys.chars()
            .map(|c| app.on_key(KeyEvent::from(KeyCode::Char(c)), &mut keymap))
            .last()
            .flatten()
    }

    #[test]
    fn keys_in_the_reader_move_its_cursor() {
        let mut app = reading(10, 5);
        app.body = Some("Dear Charles,".to_owned());

        type_keys(&mut app, "jjjw");

        assert_eq!(app.viewer.cursor(), (3, 5));
    }

    #[test]
    fn keys_the_reader_has_no_use_for_still_work() {
        let mut app = reading(10, 5);

        type_keys(&mut app, "z");

        assert!(app.zoomed);
    }

    #[test]
    fn v_in_the_reader_picks_from_the_cursor_and_y_copies_it() {
        let mut app = reading(10, 5);
        app.body = Some("Dear Charles,".to_owned());

        let effect = type_keys(&mut app, "jvjj$y");

        assert_eq!(
            effect,
            Some(Effect::Copy(
                "Subject: m0

Dear Charles,"
                    .to_owned()
            ))
        );
        assert_eq!(app.viewer.mode(), VimMode::Normal, "copying stops picking");
        assert_eq!(app.status, Some(Status::Info("Copied 3 lines".to_owned())));
    }

    #[test]
    fn text_yanked_in_the_reader_is_put_with_p_in_a_reply() {
        let mut app = reading(10, 5);
        app.body = Some("Dear Charles,".to_owned());
        type_keys(&mut app, "Gyy");

        type_keys(&mut app, "r");
        app.on_key(KeyEvent::from(KeyCode::Esc), &mut Keymap::default());
        type_keys(&mut app, "p");

        let body = app.compose.as_ref().map(|c| c.body.text());
        assert_eq!(
            body.as_deref(),
            Some(
                "
Dear Charles,"
            )
        );
    }

    #[test]
    fn the_reader_scrolls_to_keep_the_cursor_in_view() {
        let mut app = reading(4, 23);
        app.viewport.reader_width = 80;
        let body: Vec<String> = (1..=20).map(|i| format!("Line {i}")).collect();
        app.body = Some(body.join(
            "
",
        ));

        type_keys(&mut app, "10j");
        assert_eq!(app.reader_scroll, 7, "row 10 is the last in view");
        type_keys(&mut app, "gg");
        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn scrolling_the_reader_keeps_the_cursor_in_view() {
        let mut app = reading(4, 23);
        app.viewport.reader_width = 80;
        let body: Vec<String> = (1..=20).map(|i| format!("Line {i}")).collect();
        app.body = Some(body.join(
            "
",
        ));
        let wheel = |app: &mut AppState, mouse| app.on_mouse(mouse, hit(Pane::Reader, None));

        (0..3).for_each(|_| _ = wheel(&mut app, Mouse::ScrollDown));
        assert_eq!(app.reader_scroll, 9);
        assert_eq!(app.viewer.cursor(), (9, 0), "dragged down to the top line");
        wheel(&mut app, Mouse::ScrollUp);
        wheel(&mut app, Mouse::ScrollUp);
        assert_eq!(app.reader_scroll, 3);
        assert_eq!(app.viewer.cursor(), (6, 0), "dragged up to the bottom line");
    }

    #[test]
    fn another_message_starts_with_the_cursor_at_the_top() {
        let mut app = reading(10, 5);
        type_keys(&mut app, "jv");

        app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(2)));

        assert_eq!(app.selected_message, 1);
        assert_eq!(app.viewer.cursor(), (0, 0));
        assert_eq!(app.viewer.mode(), VimMode::Normal);
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
        assert_eq!(app.update(Action::Top), None, "the date header is first");
        assert_eq!(app.selected_group, Some(DateGroup::Older));
        assert_eq!(app.update(Action::SelectNext), load_body("m0"));
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
    fn showing_an_unread_message_marks_it_read() {
        let mut app = in_message_list(&["m1", "m2"]);
        let m1 = MessageId("m1".to_owned());

        let effect = app.body_loaded(m1.clone(), "body".to_owned());

        assert_eq!(effect, Some(Effect::MarkRead(m1.clone())));
        assert!(app.messages[0].is_read);
        assert_eq!(app.body_loaded(m1, "again".to_owned()), None);
    }

    #[test]
    fn a_stale_body_marks_nothing_read() {
        let mut app = in_message_list(&["m1", "m2"]);
        app.update(Action::SelectNext);

        assert_eq!(
            app.body_loaded(MessageId("m1".to_owned()), "stale".to_owned()),
            None
        );
        assert!(!app.messages[0].is_read);
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
    fn p_pins_the_selected_message_to_the_top_and_keeps_it_selected() {
        let mut app = in_message_list(&["m0", "m1"]);
        app.update(Action::SelectNext);

        let effect = key(&mut app, KeyCode::Char('p'));

        assert_eq!(
            effect,
            Some(Effect::SetPinned {
                ids: message_ids(&["m1"]),
                pinned: true
            })
        );
        assert_eq!(app.messages[0].id.0, "m1");
        assert!(app.messages[0].pinned);
        assert_eq!(app.selected().map(|m| m.id.0.as_str()), Some("m1"));
        assert!(matches!(
            app.message_rows()[0],
            MessageRow::Header {
                group: DateGroup::Pinned,
                ..
            }
        ));

        let effect = key(&mut app, KeyCode::Char('p'));

        assert_eq!(
            effect,
            Some(Effect::SetPinned {
                ids: message_ids(&["m1"]),
                pinned: false
            })
        );
        assert!(app.messages.iter().all(|m| !m.pinned));
        assert_eq!(app.selected().map(|m| m.id.0.as_str()), Some("m1"));
    }

    #[test]
    fn p_pins_every_marked_message_unless_all_are_pinned() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.messages[0].pinned = true;
        app.marked = message_ids(&["m0", "m1"]).into_iter().collect();

        let effect = app.update(Action::TogglePin);

        assert_eq!(
            effect,
            Some(Effect::SetPinned {
                ids: message_ids(&["m0", "m1"]),
                pinned: true
            })
        );
    }

    #[test]
    fn d_asks_and_y_moves_the_selected_message_to_deleted_items() {
        let mut app = in_message_list(&["m0", "m1"]);

        assert_eq!(key(&mut app, KeyCode::Char('d')), None);
        assert!(app.confirm().is_some());

        let effect = key(&mut app, KeyCode::Char('y'));
        assert_eq!(
            effect,
            Some(Effect::Delete {
                ids: message_ids(&["m0"]),
                permanent: false
            })
        );
        assert_eq!(app.confirm(), None);
    }

    #[test]
    fn shift_d_deletes_the_marked_messages_for_good_after_enter() {
        let mut app = in_message_list(&["m0", "m1", "m2"]);
        app.update(Action::Bottom);
        app.update(Action::ToggleMark);
        app.update(Action::Top);
        app.update(Action::SelectNext);
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

        assert_eq!(app.confirm(), None);
        assert_eq!(app.selected_message, 0);
    }

    #[test]
    fn d_in_the_folder_pane_deletes_the_folder_not_a_message() {
        let mut app = in_message_list(&["m0"]);
        app.focus = Pane::Folders;
        app.update(Action::Delete);
        assert!(matches!(app.confirm(), Some(Confirm::DeleteFolder { .. })));
    }

    #[test]
    fn there_is_nothing_to_delete_in_an_empty_folder() {
        let mut app = in_message_list(&[]);
        app.update(Action::Delete);
        assert_eq!(app.confirm(), None);
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
        assert_eq!(app.dialog, Some(Dialog::Help));

        assert_eq!(key(&mut app, KeyCode::Char('j')), None);
        assert_eq!(app.dialog, None);
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
        assert_eq!(app.status, Some(Status::Busy("Sending…".to_owned())));
    }

    fn two_accounts() -> AppState {
        let mut app = AppState {
            accounts: vec!["me@outlook.com".to_owned(), "me@work.example".to_owned()],
            signatures: vec![String::new(), String::new()],
            ..Default::default()
        };
        app.set_account_folders(0, vec![folder("a")]);
        app.set_account_folders(1, vec![folder("b")]);
        app
    }

    #[test]
    fn shift_s_writes_the_signature_of_the_account_being_looked_at() {
        let mut app = two_accounts();
        // The rows are: account 0, a, account 1, b.
        app.selected_folder = 3;

        key(&mut app, KeyCode::Char('S'));
        for c in "Ada".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let effect = ctrl_s(&mut app);

        assert_eq!(
            effect,
            Some(Effect::SaveSignature {
                account: 1,
                text: "Ada".to_owned()
            })
        );
        assert!(app.signature.is_none());
        assert_eq!(app.account_for(&effect.unwrap()), 1);
        assert_eq!(app.status, Some(Status::Info("Signature saved".to_owned())));
    }

    #[test]
    fn a_new_message_starts_with_its_accounts_signature() {
        let mut app = two_accounts();
        app.signatures[1] = "Ada".to_owned();

        app.selected_folder = 3;
        app.update(Action::Compose);
        assert_eq!(
            app.compose.as_ref().unwrap().body.text(),
            "

Ada"
        );

        app.compose = None;
        app.selected_folder = 1;
        app.update(Action::Compose);
        assert_eq!(app.compose.as_ref().unwrap().body.text(), "");
    }

    #[test]
    fn closing_a_changed_signature_asks_first() {
        let mut app = two_accounts();
        app.update(Action::EditSignature);

        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.confirm(), Some(&Confirm::DiscardSignature));
        key(&mut app, KeyCode::Char('y'));

        assert!(app.signature.is_none());
        assert_eq!(app.signatures[0], "", "nothing was saved");
    }

    fn alt_gr(app: &mut AppState, c: char) {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL | KeyModifiers::ALT);
        app.on_key(key, &mut Keymap::default());
    }

    #[test]
    fn alt_gr_characters_are_typed_in_the_header_and_the_body() {
        let mut app = AppState::default();
        app.update(Action::Compose);

        for c in "ada".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        alt_gr(&mut app, '@');
        for c in "example.com".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Char('i'));
        for c in ['{', '€', '}', '|', '\\'] {
            alt_gr(&mut app, c);
        }

        let compose = app.compose.as_ref().unwrap();
        assert_eq!(compose.to.text(), "ada@example.com");
        assert_eq!(compose.body.text(), "{€}|\\");
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
        app.mail_sent(Instant::now());
        assert!(app.compose.is_none());
        assert_eq!(app.status, Some(Status::Info("Sent".to_owned())));
    }

    /// A reply ready to send, after `sends` messages went out just now.
    fn replying_after(sends: usize) -> AppState {
        let mut app = in_message_list(&["m0"]);
        app.messages[0].from_addr = "ada@example.com".to_owned();
        for _ in 0..sends {
            app.mail_sent(Instant::now());
        }
        app.update(Action::Reply);
        key(&mut app, KeyCode::Char('x'));
        app
    }

    #[test]
    fn a_few_sends_in_a_row_go_straight_out() {
        let mut app = replying_after(PACE_LIMIT - 1);

        assert!(matches!(ctrl_s(&mut app), Some(Effect::Send(_))));
        assert_eq!(app.confirm(), None);
    }

    #[test]
    fn a_burst_of_sends_asks_first_and_y_sends_anyway() {
        let mut app = replying_after(PACE_LIMIT);

        assert_eq!(ctrl_s(&mut app), None);
        assert!(matches!(
            app.confirm(),
            Some(Confirm::SendAnyway {
                sent_lately: PACE_LIMIT,
                ..
            })
        ));

        assert!(matches!(
            key(&mut app, KeyCode::Char('y')),
            Some(Effect::Send(_))
        ));
        assert_eq!(app.status, Some(Status::Busy("Sending…".to_owned())));
    }

    #[test]
    fn waiting_instead_keeps_the_message_open_to_send_later() {
        let mut app = replying_after(PACE_LIMIT);
        ctrl_s(&mut app);

        assert_eq!(key(&mut app, KeyCode::Esc), None);

        let compose = app.compose.as_ref().unwrap();
        assert!(!compose.sending);
        assert_eq!(compose.body.text(), "x");
    }

    #[test]
    fn sends_older_than_the_window_do_not_count() {
        let mut app = AppState::default();
        let start = Instant::now();
        for _ in 0..PACE_LIMIT {
            app.mail_sent(start);
        }

        assert_eq!(app.sent_lately(0, start + PACE_WINDOW), 0);
        app.mail_sent(start + PACE_WINDOW);
        assert_eq!(app.sent.len(), 1, "old ones are forgotten");
    }

    #[test]
    fn a_blocked_send_explains_itself_and_keeps_the_message_to_retry() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Reply);
        key(&mut app, KeyCode::Char('x'));
        ctrl_s(&mut app);

        app.send_blocked("ErrorMessageSubmissionBlocked: blocked".to_owned());

        assert!(matches!(app.dialog, Some(Dialog::Blocked(_))));
        assert!(app.compose.as_ref().is_some_and(|c| !c.sending));
        assert_eq!(key(&mut app, KeyCode::Char('y')), None);
        assert_eq!(app.dialog, None, "any key closes it");
        assert_eq!(
            app.compose.as_ref().map(|c| c.body.text()),
            Some("x".to_owned()),
            "the key only closed the dialog"
        );
    }

    #[test]
    fn closing_a_written_message_asks_first() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Reply);
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Esc);

        key(&mut app, KeyCode::Esc);
        assert_eq!(app.confirm(), Some(&Confirm::Discard));
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

        // Row 0 is the date header.
        let effect = app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(3)));

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
        app.dialog = Some(Dialog::Help);

        assert_eq!(
            app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(1))),
            None
        );

        assert_eq!(app.dialog, None);
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

    fn after(app: &AppState, seconds: u64) -> Instant {
        app.status_since.unwrap() + Duration::from_secs(seconds)
    }

    #[test]
    fn news_makes_way_for_the_key_hints_after_a_few_seconds() {
        let mut app = AppState::default();
        app.show_info("Sent");

        app.expire_status(after(&app, 3));
        assert_eq!(app.status, Some(Status::Info("Sent".to_owned())));
        app.expire_status(after(&app, 4));
        assert_eq!(app.status, None);
    }

    #[test]
    fn errors_stay_longer_to_be_read() {
        let mut app = AppState::default();
        app.show_error("offline".to_owned());

        app.expire_status(after(&app, 7));
        assert!(app.status.is_some());
        app.expire_status(after(&app, 8));
        assert_eq!(app.status, None);
    }

    #[test]
    fn work_in_progress_stays_until_its_outcome_replaces_it() {
        let mut app = in_message_list(&["m0"]);
        key(&mut app, KeyCode::Char('d'));
        key(&mut app, KeyCode::Char('y'));

        app.expire_status(after(&app, 60));
        assert_eq!(
            app.status,
            Some(Status::Busy("Deleting 1 message…".to_owned()))
        );

        app.messages_deleted(&message_ids(&["m0"]));
        app.expire_status(after(&app, 4));
        assert_eq!(app.status, None, "the outcome then goes by itself");
    }

    fn every_minute() -> AppState {
        let mut app = AppState::default();
        app.sync.every = Some(Duration::from_secs(60));
        app
    }

    #[test]
    fn mail_is_checked_again_a_minute_after_the_last_sync_finished() {
        let mut app = every_minute();
        let start = Instant::now();
        assert_eq!(app.start_sync(), Some(Effect::Sync));
        assert_eq!(app.sync_if_due(start + Duration::from_secs(600)), None);

        app.sync_finished(start);

        assert_eq!(app.sync_if_due(start + Duration::from_secs(59)), None);
        assert_eq!(
            app.sync_if_due(start + Duration::from_secs(60)),
            Some(Effect::Sync)
        );
        assert_eq!(
            app.sync_if_due(start + Duration::from_secs(61)),
            None,
            "one sync at a time"
        );
    }

    #[test]
    fn without_an_interval_mail_is_checked_only_when_asked() {
        let mut app = AppState::default();
        let start = Instant::now();
        app.start_sync();
        app.sync_finished(start);

        assert_eq!(app.sync_if_due(start + Duration::from_secs(3600)), None);
        assert_eq!(app.update(Action::Refresh), Some(Effect::Sync));
    }

    #[test]
    fn f5_says_it_is_syncing_then_that_mail_is_up_to_date() {
        let mut app = AppState::default();

        assert_eq!(app.update(Action::Refresh), Some(Effect::Sync));
        assert_eq!(app.status, Some(Status::Busy("Syncing…".to_owned())));
        assert_eq!(app.update(Action::Refresh), None, "already syncing");

        app.sync_finished(Instant::now());
        assert_eq!(app.status, Some(Status::Info("Up to date".to_owned())));
    }

    #[test]
    fn an_error_during_a_requested_sync_is_not_covered_up() {
        let mut app = AppState::default();
        app.update(Action::Refresh);
        app.show_error("offline".to_owned());

        app.sync_finished(Instant::now());

        assert_eq!(app.status, Some(Status::Error("offline".to_owned())));
    }

    #[test]
    fn sending_checks_again_soon_so_the_sent_message_shows() {
        let mut app = every_minute();
        let start = Instant::now();
        app.start_sync();
        app.mail_sent(start);
        app.sync_finished(start + Duration::from_secs(1));

        assert_eq!(
            app.sync_if_due(start + Duration::from_secs(5)),
            Some(Effect::Sync),
            "well before the minute is up"
        );
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

    /// Messages received now, then messages from long ago, in two groups.
    fn today_and_older(today: &[&str], older: &[&str]) -> Vec<Message> {
        let now = jiff::Timestamp::now();
        let today = today.iter().map(|id| Message {
            received_at: now,
            ..message(id)
        });
        today.chain(older.iter().map(|id| message(id))).collect()
    }

    #[test]
    fn moving_steps_onto_date_headers_too() {
        let mut app = with_folders(&["a"]);
        app.focus = Pane::Messages;
        assert_eq!(
            app.set_messages(today_and_older(&["t1"], &["o1"])),
            load_body("t1")
        );

        assert_eq!(app.update(Action::SelectNext), None);
        assert_eq!(app.selected_group, Some(DateGroup::Older));
        assert_eq!(app.selected(), None);
        assert_eq!(app.update(Action::SelectNext), load_body("o1"));
    }

    #[test]
    fn moving_skips_the_mail_of_folded_groups() {
        let mut app = with_folders(&["a"]);
        app.focus = Pane::Messages;
        app.collapsed_groups.insert(DateGroup::Today);

        assert_eq!(
            app.set_messages(today_and_older(&["t1", "t2"], &["o1"])),
            None,
            "the folded group's header is selected"
        );
        assert_eq!(app.selected_group, Some(DateGroup::Today));
        app.update(Action::SelectNext);
        assert_eq!(app.update(Action::SelectNext), load_body("o1"));
    }

    #[test]
    fn a_selected_header_stays_selected_after_a_sync() {
        let mut app = with_folders(&["a"]);
        app.focus = Pane::Messages;
        app.set_messages(today_and_older(&["t1"], &["o1"]));
        app.update(Action::SelectNext);

        app.refresh_messages(today_and_older(&["t0", "t1"], &["o1"]));

        assert_eq!(app.selected_group, Some(DateGroup::Older));
        assert_eq!(app.selected_message, 2);
    }

    #[test]
    fn left_folds_the_group_of_the_selected_message_onto_its_header() {
        let mut app = with_folders(&["a"]);
        app.focus = Pane::Messages;
        app.set_messages(today_and_older(&["t1", "t2"], &["o1"]));
        app.update(Action::SelectNext);
        app.body = Some("t2".to_owned());

        app.update(Action::Collapse);

        assert!(app.collapsed_groups.contains(&DateGroup::Today));
        assert_eq!(app.selected_group, Some(DateGroup::Today));
        assert_eq!(app.body, None, "the reader no longer shows t2");
        assert_eq!(app.update(Action::SelectNext), None, "onto Older");
        assert_eq!(app.update(Action::SelectNext), load_body("o1"));
    }

    #[test]
    fn right_unfolds_a_header_then_steps_into_it() {
        let mut app = with_folders(&["a"]);
        app.focus = Pane::Messages;
        app.collapsed_groups.insert(DateGroup::Today);
        app.set_messages(today_and_older(&["t1"], &["o1"]));

        assert_eq!(app.update(Action::Expand), None);
        assert!(app.collapsed_groups.is_empty());
        assert_eq!(app.update(Action::Expand), load_body("t1"));
    }

    #[test]
    fn right_in_the_reader_shows_the_next_mail_past_a_date_header() {
        let mut app = with_folders(&["a"]);
        app.set_messages(today_and_older(&["t1"], &["o1"]));
        app.focus = Pane::Reader;
        app.reader_scroll = 5;

        assert_eq!(app.update(Action::Expand), load_body("o1"));
        assert_eq!(app.focus, Pane::Reader);
        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn left_in_the_reader_shows_the_previous_mail() {
        let mut app = with_folders(&["a"]);
        app.set_messages(today_and_older(&["t1"], &["o1"]));
        app.focus = Pane::Reader;
        app.update(Action::Expand);

        assert_eq!(app.update(Action::Collapse), load_body("t1"));
        assert_eq!(app.focus, Pane::Reader);
    }

    #[test]
    fn the_reader_stays_on_the_first_and_last_mail() {
        let mut app = with_folders(&["a"]);
        app.set_messages(today_and_older(&["t1"], &["o1"]));
        app.focus = Pane::Reader;
        app.body = Some("t1".to_owned());

        assert_eq!(app.update(Action::Collapse), None);
        assert_eq!(app.body.as_deref(), Some("t1"), "still showing t1");
        app.update(Action::Expand);
        assert_eq!(app.update(Action::Expand), None);
        assert_eq!(app.selected().map(|m| m.id.0.as_str()), Some("o1"));
    }

    #[test]
    fn the_zoomed_reader_skips_the_mail_of_folded_groups() {
        let mut app = with_folders(&["a"]);
        app.collapsed_groups.insert(DateGroup::Older);
        app.set_messages(today_and_older(&["t1"], &["o1", "o2"]));
        app.focus = Pane::Reader;
        app.zoomed = true;

        assert_eq!(app.update(Action::Expand), None);
        assert_eq!(app.selected().map(|m| m.id.0.as_str()), Some("t1"));
        assert_eq!(app.zoomed_pane(), Some(Pane::Reader));
    }

    #[test]
    fn enter_on_a_header_folds_and_unfolds_it() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Top);

        app.update(Action::Open);
        assert!(app.collapsed_groups.contains(&DateGroup::Older));
        app.update(Action::Open);
        assert!(app.collapsed_groups.is_empty());
        assert_eq!(app.focus, Pane::Messages);
    }

    #[test]
    fn a_header_cannot_be_replied_to_or_deleted() {
        let mut app = in_message_list(&["m0"]);
        app.update(Action::Top);

        app.update(Action::Reply);
        app.update(Action::Delete);

        assert!(app.compose.is_none());
        assert_eq!(app.confirm(), None);
    }

    #[test]
    fn a_click_on_a_header_folds_and_unfolds_its_group() {
        let mut app = in_message_list(&["m0", "m1"]);

        assert_eq!(
            app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(0))),
            None
        );
        assert!(app.collapsed_groups.contains(&DateGroup::Older));
        assert_eq!(app.selected_group, Some(DateGroup::Older));

        app.on_mouse(Mouse::Click, hit(Pane::Messages, Some(0)));
        assert!(app.collapsed_groups.is_empty());
    }
}
