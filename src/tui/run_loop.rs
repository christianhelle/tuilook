//! What the event loop does with what the terminal and the cores send it.
//! The terminal itself stays with the caller, so all of this runs in tests.

use anyhow::Result;
use ratatui::{
    crossterm::event::{Event as TermEvent, KeyEventKind, MouseButton, MouseEventKind},
    layout::Rect,
};
use tokio::sync::mpsc;

use crate::model::Event;
use crate::tui::{
    app::{AppState, Mouse},
    keymap::Keymap,
    mailboxes::Mailboxes,
    ui,
};

/// Shows everything the cores have reported since the last call, one list of
/// events per account. Never waits for more.
pub fn take_core_events(
    app: &mut AppState,
    mailboxes: &mut Mailboxes,
    events: &mut [mpsc::Receiver<Event>],
) -> Result<()> {
    for (account, events) in events.iter_mut().enumerate() {
        while let Ok(event) = events.try_recv() {
            mailboxes.on_event(app, account, event)?;
        }
    }
    Ok(())
}

/// Hands a terminal event to the app, measuring the panes in `area` first so
/// that paging moves by what is on screen, and performs the IO it asks for.
pub fn on_terminal_event(
    app: &mut AppState,
    mailboxes: &mut Mailboxes,
    keymap: &mut Keymap,
    area: Rect,
    event: TermEvent,
) -> Result<()> {
    app.viewport = ui::viewport(area, app);
    let effect = match event {
        // Windows also reports key releases.
        TermEvent::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key, keymap),
        TermEvent::Mouse(mouse) => {
            let action = match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => Mouse::Click,
                MouseEventKind::ScrollUp => Mouse::ScrollUp,
                MouseEventKind::ScrollDown => Mouse::ScrollDown,
                _ => return Ok(()),
            };
            let hit = ui::hit(area, app, mouse.column, mouse.row);
            app.on_mouse(action, hit)
        }
        _ => return Ok(()),
    };
    mailboxes.apply(app, effect)
}
