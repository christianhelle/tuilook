/// Something the user asked for. The UI applies it to its own state and
/// forwards anything that needs IO to core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    FocusNextPane,
    FocusPrevPane,
    SelectNext,
    SelectPrev,
    Open,
    Quit,
}

/// Something core tells the UI happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Synced,
    Toast(String),
    Error(String),
}
