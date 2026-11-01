//! Picking a file to attach: a folder's contents, narrowed by typing part
//! of a name, walked into and out of.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::input::TextInput;

/// Where to pick the first file to attach from: the documents folder, or
/// else the home folder, or else the folder tuilook was started in.
pub fn start_dir() -> PathBuf {
    let dirs = directories::UserDirs::new();
    dirs.as_ref()
        .and_then(|d| d.document_dir().map(PathBuf::from))
        .or_else(|| dirs.as_ref().map(|d| d.home_dir().to_path_buf()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// What a key asks for while picking a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    None,
    Close,
    /// Attach the file at this path.
    Pick(PathBuf),
}

/// One row of a folder: the folder above it, a folder in it, or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub folder: bool,
    /// In bytes; nothing for a folder.
    pub size: u64,
}

impl Entry {
    fn up() -> Self {
        Self {
            name: "..".to_owned(),
            folder: true,
            size: 0,
        }
    }

    /// The name as shown, a folder's ending in `/`.
    pub fn label(&self) -> String {
        if self.folder && self.name != ".." {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePicker {
    /// The folder shown.
    pub dir: PathBuf,
    /// `..` when there is a folder above, then the folders, then the files.
    pub entries: Vec<Entry>,
    /// Part of a name, to narrow the entries shown.
    pub filter: TextInput,
    /// The picked one of the entries shown.
    pub selected: usize,
    /// Why the folder could not be read, if it could not.
    pub error: Option<String>,
}

impl FilePicker {
    /// Shows what is in `dir`.
    pub fn open(dir: PathBuf) -> Self {
        let mut picker = Self {
            dir,
            entries: vec![],
            filter: TextInput::default(),
            selected: 0,
            error: None,
        };
        picker.read();
        picker
    }

    /// Reads the folder shown afresh, clearing the filter.
    fn read(&mut self) {
        self.filter = TextInput::default();
        self.selected = 0;
        self.error = None;
        self.entries = self.dir.parent().map(|_| Entry::up()).into_iter().collect();
        let mut found = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| {
                    // Following links, so a link to a folder opens it.
                    let meta = std::fs::metadata(entry.path()).ok();
                    Entry {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        folder: meta.as_ref().is_some_and(|m| m.is_dir()),
                        size: meta.filter(|m| m.is_file()).map_or(0, |m| m.len()),
                    }
                })
                .collect(),
            Err(e) => {
                self.error = Some(e.to_string());
                vec![]
            }
        };
        found.sort_by_key(|e| (!e.folder, e.name.to_lowercase()));
        self.entries.extend(found);
    }

    /// The entries whose name has the filter in it, as indexes into
    /// `entries`. `..` shows only while nothing is typed.
    pub fn choices(&self) -> Vec<usize> {
        let filter = self.filter.text().to_lowercase();
        (0..self.entries.len())
            .filter(|&i| {
                let name = &self.entries[i].name;
                if filter.is_empty() {
                    return true;
                }
                name != ".." && name.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// The entry picked, if any is shown.
    pub fn picked(&self) -> Option<&Entry> {
        let index = *self.choices().get(self.selected)?;
        Some(&self.entries[index])
    }

    /// Typing narrows the entries, ↑ ↓ pick one, Enter opens a folder or
    /// attaches a file, Backspace with nothing typed goes up and Esc gives
    /// up.
    pub fn key(&mut self, key: KeyEvent) -> Request {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Request::Close,
            KeyCode::Enter => return self.enter(),
            KeyCode::Backspace if self.filter.text().is_empty() => self.up(),
            KeyCode::Down | KeyCode::Tab => self.step(1),
            KeyCode::Char('n') if ctrl => self.step(1),
            KeyCode::Up | KeyCode::BackTab => self.step(-1),
            KeyCode::Char('p') if ctrl => self.step(-1),
            _ => {
                let before = self.filter.text().to_owned();
                if self.filter.key(key) && self.filter.text() != before {
                    self.selected = 0;
                }
            }
        }
        Request::None
    }

    fn enter(&mut self) -> Request {
        let Some(entry) = self.picked().cloned() else {
            return Request::None;
        };
        if entry.name == ".." {
            self.up();
        } else if entry.folder {
            self.dir.push(&entry.name);
            self.read();
        } else {
            return Request::Pick(self.dir.join(&entry.name));
        }
        Request::None
    }

    /// Shows the folder above, picking the folder just left.
    fn up(&mut self) {
        let Some(parent) = self.dir.parent().map(PathBuf::from) else {
            return;
        };
        let left = self
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.dir = parent;
        self.read();
        if let Some(left) = left {
            let choices = self.choices();
            if let Some(at) = choices.iter().position(|&i| self.entries[i].name == left) {
                self.selected = at;
            }
        }
    }

    /// Picks the entry `delta` rows away, stopping at the ends.
    fn step(&mut self, delta: isize) {
        let last = self.choices().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    use super::*;

    /// A folder holding `b.txt`, `A.pdf` and folders `docs` and `Zed`, with
    /// `docs/inner.md` inside.
    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::write(dir.path().join("A.pdf"), "a").unwrap();
        fs::create_dir(dir.path().join("docs")).unwrap();
        fs::create_dir(dir.path().join("Zed")).unwrap();
        fs::write(dir.path().join("docs").join("inner.md"), "#").unwrap();
        dir
    }

    fn shown(picker: &FilePicker) -> Vec<String> {
        picker
            .choices()
            .into_iter()
            .map(|i| picker.entries[i].label())
            .collect()
    }

    fn press(picker: &mut FilePicker, codes: &[KeyCode]) -> Request {
        let mut request = Request::None;
        for &code in codes {
            request = picker.key(KeyEvent::from(code));
        }
        request
    }

    fn typed(text: &str) -> Vec<KeyCode> {
        text.chars().map(KeyCode::Char).collect()
    }

    #[test]
    fn folders_come_first_then_files_each_by_name() {
        let dir = folder();

        let picker = FilePicker::open(dir.path().to_path_buf());

        assert_eq!(shown(&picker), ["..", "docs/", "Zed/", "A.pdf", "b.txt"]);
    }

    #[test]
    fn typing_narrows_to_names_with_it_in() {
        let dir = folder();
        let mut picker = FilePicker::open(dir.path().to_path_buf());

        press(&mut picker, &typed("D"));

        assert_eq!(shown(&picker), ["docs/", "Zed/", "A.pdf"]);
    }

    #[test]
    fn enter_on_a_folder_opens_it_and_backspace_goes_back_up() {
        let dir = folder();
        let mut picker = FilePicker::open(dir.path().to_path_buf());

        press(&mut picker, &[KeyCode::Down, KeyCode::Enter]);
        assert_eq!(picker.dir, dir.path().join("docs"));
        assert_eq!(shown(&picker), ["..", "inner.md"]);

        press(&mut picker, &[KeyCode::Backspace]);
        assert_eq!(picker.dir, dir.path());
        assert_eq!(picker.picked().map(Entry::label), Some("docs/".to_owned()));
    }

    #[test]
    fn enter_on_a_file_picks_it() {
        let dir = folder();
        let mut picker = FilePicker::open(dir.path().to_path_buf());

        let request = press(
            &mut picker,
            &[typed("b.t").as_slice(), &[KeyCode::Enter]].concat(),
        );

        assert_eq!(request, Request::Pick(dir.path().join("b.txt")));
    }

    #[test]
    fn backspace_deletes_typed_text_before_going_up() {
        let dir = folder();
        let mut picker = FilePicker::open(dir.path().join("docs"));

        press(&mut picker, &[KeyCode::Char('x'), KeyCode::Backspace]);

        assert_eq!(picker.dir, dir.path().join("docs"));
        assert_eq!(press(&mut picker, &[KeyCode::Esc]), Request::Close);
    }

    #[test]
    fn a_folder_that_cannot_be_read_says_why() {
        let dir = folder();

        let picker = FilePicker::open(dir.path().join("missing"));

        assert!(picker.error.is_some());
        assert_eq!(shown(&picker), [".."]);
    }

    #[test]
    fn ctrl_a_while_writing_picks_a_file_to_attach_from_the_last_folder() {
        use crate::model::ComposeKind;
        use crate::tui::{app::AppState, compose::Compose, dialog::Dialog, keymap::Keymap};
        use ratatui::crossterm::event::KeyModifiers;

        let dir = folder();
        let mut app = AppState {
            attach_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        app.start_compose(Compose::new(ComposeKind::New, None));
        let mut keymap = Keymap::default();
        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        app.on_key(ctrl_a, &mut keymap);
        assert!(matches!(app.dialog, Some(Dialog::Files(_))));
        for code in typed("b.t").into_iter().chain([KeyCode::Enter]) {
            app.on_key(KeyEvent::from(code), &mut keymap);
        }

        assert_eq!(app.dialog, None);
        let compose = app.compose.as_ref().unwrap();
        assert_eq!(compose.attachments, [dir.path().join("b.txt")]);

        // The next file is picked from where the last one was.
        app.on_key(ctrl_a, &mut keymap);
        let Some(Dialog::Files(picker)) = &app.dialog else {
            panic!("no file picker");
        };
        assert_eq!(picker.dir, dir.path());
    }
}
