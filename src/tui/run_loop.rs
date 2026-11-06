//! What the event loop does with what the terminal and the cores send it.
//! The terminal itself stays with the caller, so all of this runs in tests.

use anyhow::Result;
use ratatui::{
    crossterm::event::{Event as TermEvent, KeyEventKind, MouseButton, MouseEventKind},
    layout::Rect,
};
use semver::Version;
use tokio::sync::mpsc;

use crate::model::Event;
use crate::tui::{
    account_manager::Login,
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

/// What checking for and installing a newer tuilook comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateNews {
    /// A check found this newer release.
    Available(Version),
    /// This release replaced the running binary.
    Installed(Version),
    /// Asked to update, the running release turned out to be the latest.
    UpToDate,
    /// Updating failed, for this reason.
    Failed(String),
}

/// Shows what checking for and installing updates has come to since the
/// last call. Never waits for news, so a slow network cannot hold up the UI.
pub fn take_update(app: &mut AppState, updates: &mut mpsc::Receiver<UpdateNews>) {
    while let Ok(news) = updates.try_recv() {
        match news {
            UpdateNews::Available(version) => app.update = Some(version),
            UpdateNews::Installed(version) => {
                app.update = None;
                app.show_info(format!(
                    "Installed tuilook {version}; restart otlk to use it"
                ));
                app.installed = Some(version);
            }
            UpdateNews::UpToDate => {
                app.update = None;
                app.show_info("tuilook is up to date");
            }
            UpdateNews::Failed(reason) => app.show_error(format!("Could not update: {reason}")),
        }
    }
}

/// What signing an account in comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountNews {
    /// What Microsoft says to do to sign in.
    Code(String),
    /// Signed in as `address` with `login`, and saved.
    SignedIn { address: String, login: Login },
    /// Signing in failed, for this reason.
    Failed(String),
}

/// Shows what signing accounts in has come to since the last call, without
/// waiting for more.
pub fn take_account_news(app: &mut AppState, news: &mut mpsc::UnboundedReceiver<AccountNews>) {
    while let Ok(said) = news.try_recv() {
        match said {
            AccountNews::Code(message) => app.sign_in_code(message),
            AccountNews::SignedIn { address, login } => app.signed_in(address, login),
            AccountNews::Failed(reason) => app.sign_in_failed(&reason),
        }
    }
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
