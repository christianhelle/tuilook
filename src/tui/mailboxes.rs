//! Each account's mailbox: its cache, read here on the UI thread, and the
//! core that syncs it. The mailboxes perform the IO the app asks for and
//! show the app what each core reports.

use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use tokio::sync::mpsc;

use crate::accounts::Accounts;
use crate::core::{Command, calendar_window};
use crate::model::{Event, FolderId, Message};
use crate::store::Store;
use crate::tui::{
    app::{AppState, Effect},
    clipboard,
    search::SEARCH_LIMIT,
};

/// One account's cache, and the channel to the core that syncs it.
pub struct Mailbox {
    pub store: Store,
    /// Never full, so the UI thread cannot stall behind a busy core and stop
    /// taking the events that core is waiting to hand over.
    pub commands: mpsc::UnboundedSender<Command>,
}

/// Every account's mailbox, in the order of the app's accounts.
pub struct Mailboxes {
    mailboxes: Vec<Mailbox>,
    /// Where signatures are kept, if anywhere.
    accounts_file: Option<PathBuf>,
    /// Where copied text goes, as an OSC 52 sequence: the terminal.
    clipboard: Box<dyn Write>,
}

impl Mailboxes {
    pub fn new(
        mailboxes: Vec<Mailbox>,
        accounts_file: Option<PathBuf>,
        clipboard: Box<dyn Write>,
    ) -> Self {
        Self {
            mailboxes,
            accounts_file,
            clipboard,
        }
    }

    pub fn len(&self) -> usize {
        self.mailboxes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mailboxes.is_empty()
    }

    /// `account`'s cache.
    pub fn store(&self, account: usize) -> &Store {
        &self.mailboxes[account].store
    }

    /// Shows what `account`'s core did, and performs the IO that leads to.
    pub fn on_event(&mut self, app: &mut AppState, account: usize, event: Event) -> Result<()> {
        let effect = self.shown(app, account, event)?;
        self.apply(app, effect)
    }

    /// Shows what `account`'s core did.
    fn shown(&self, app: &mut AppState, account: usize, event: Event) -> Result<Option<Effect>> {
        let store = &self.mailboxes[account].store;
        Ok(match event {
            Event::FoldersSynced => app.set_account_folders(account, store.folders()?),
            Event::SyncFinished => {
                app.sync_finished(Instant::now());
                None
            }
            Event::FolderSynced(id) => {
                // Unread counts may have changed along with the messages.
                let effect = app.set_account_folders(account, store.folders()?);
                if let Some(query) = app.results_query() {
                    Some(Effect::Search(query.to_owned()))
                } else {
                    refresh_shown(app, store, &id)?.or(effect)
                }
            }
            Event::OlderLoaded {
                folder,
                unread_only,
                more,
            } => {
                app.older_loaded(folder.clone(), unread_only, more);
                refresh_shown(app, store, &folder)?
            }
            Event::FolderDeleted(folder) => {
                app.folder_deleted(&folder);
                None
            }
            Event::FolderCreated(name) => {
                app.show_info(format!("Created {name}"));
                None
            }
            Event::FolderRenamed { name, .. } => {
                app.show_info(format!("Renamed to {name}"));
                None
            }
            Event::BodyLoaded {
                message,
                text,
                attachments,
            } => app.body_loaded(message, text, attachments),
            Event::InviteLoaded { message, invite } => {
                app.invite_loaded(message, *invite);
                None
            }
            Event::InviteAnswered { message, answer } => {
                app.invite_answered(&message, answer);
                None
            }
            Event::MessagesDeleted(ids) => app.messages_deleted(&ids),
            Event::MessagesMoved { ids, to } => app.messages_moved(&ids, &to),
            Event::AttachmentSaved(path) => {
                app.show_info(format!("Saved to {}", path.display()));
                None
            }
            Event::Sent => {
                app.mail_sent(Instant::now());
                None
            }
            Event::SendFailed(error) => {
                app.send_failed(&error);
                None
            }
            Event::SendBlocked(said) => {
                app.send_blocked(said);
                None
            }
            Event::CalendarSynced => app.calendar_synced(account),
            Event::AppointmentSaved => {
                app.appointment_saved();
                None
            }
            Event::AppointmentDeleted(_) => app.appointment_deleted(account),
            Event::AppointmentNotSaved(why) => {
                app.appointment_not_saved();
                show_error(
                    app,
                    account,
                    format!("could not save the appointment: {why}"),
                );
                None
            }
            Event::Error(error) => {
                show_error(app, account, error);
                None
            }
        })
    }
}

/// Shows an error from `account`, naming it when there are several.
fn show_error(app: &mut AppState, account: usize, error: String) {
    match app.accounts.get(account).filter(|_| app.accounts.len() > 1) {
        Some(address) => app.show_error(format!("{address}: {error}")),
        None => app.show_error(error),
    }
}

/// Shows `folder`'s messages from the store again, if it is the one shown.
fn refresh_shown(app: &mut AppState, store: &Store, folder: &FolderId) -> Result<Option<Effect>> {
    if app.results_query().is_some() || app.shown_folder().as_ref() != Some(folder) {
        return Ok(None);
    }
    let messages = if app.folder_pane.shows_unread() {
        store.unread_in(folder, &app.kept_read())?
    } else {
        store.messages_in(folder)?
    };
    Ok(app.refresh_messages(messages))
}

impl Mailboxes {
    /// Performs the IO an update asked for, in the account it is for, and any
    /// IO it leads to. Once there is none, older mail is fetched if the message
    /// list has reached its end.
    pub fn apply(&mut self, app: &mut AppState, mut effect: Option<Effect>) -> Result<()> {
        let mailboxes = &self.mailboxes;
        while let Some(next) = effect.or_else(|| app.older_wanted()) {
            let Mailbox { store, commands } = &mailboxes[app.account_for(&next)];
            effect = match next {
                Effect::Core { command, .. } => {
                    commands.send(command)?;
                    None
                }
                Effect::Cores(work) => {
                    for (account, command) in work {
                        mailboxes[account].commands.send(command)?;
                    }
                    None
                }
                Effect::LoadFolder(id) => app.set_messages(store.messages_in(&id)?),
                Effect::LoadUnread(id) => app.set_messages(store.unread_in(&id, &[])?),
                Effect::Sync => {
                    for mailbox in mailboxes {
                        mailbox.commands.send(Command::SyncAll)?;
                        mailbox.commands.send(Command::SyncCalendar)?;
                    }
                    None
                }
                Effect::SaveAttachments {
                    message,
                    attachments,
                } => {
                    let save = Command::SaveAttachments {
                        message,
                        attachments,
                        dir: downloads(),
                    };
                    commands.send(save)?;
                    None
                }
                Effect::Search(query) => app.search_results(search_all(mailboxes, &query)?),
                Effect::SaveSignature { account, text } => {
                    let saved = self.accounts_file.as_deref().map_or(Ok(()), |path| {
                        save_signature(path, &app.accounts[account], &text)
                    });
                    if let Err(e) = saved {
                        app.show_error(format!("Could not save the signature: {e}"));
                    }
                    None
                }
                Effect::Copy(text) => {
                    self.clipboard
                        .write_all(clipboard::osc52(&text).as_bytes())?;
                    self.clipboard.flush()?;
                    None
                }
                Effect::LoadAgenda { account, sync } => {
                    let window = calendar_window(jiff::Timestamp::now());
                    // Opened laid out beyond the calendar kept around today.
                    let fetch = app
                        .screen
                        .agenda_mut()
                        .filter(|_| sync)
                        .and_then(|agenda| agenda.take_moved(window))
                        .flatten();
                    load_calendar(app, store, account, window)?;
                    if sync {
                        commands.send(Command::SyncCalendar)?;
                    }
                    if let Some((from, to)) = fetch {
                        commands.send(Command::SyncCalendarBetween { from, to })?;
                    }
                    None
                }
                Effect::ShowCalendarRange { account, fetch } => {
                    load_calendar(app, store, account, calendar_window(jiff::Timestamp::now()))?;
                    if let Some((from, to)) = fetch {
                        commands.send(Command::SyncCalendarBetween { from, to })?;
                    }
                    None
                }
            };
        }
        Ok(())
    }
}

/// Shows `account`'s cached calendar in its open agenda: what is kept around
/// today in `window`, and anything laid out beyond it.
fn load_calendar(
    app: &mut AppState,
    store: &Store,
    account: usize,
    window: (jiff::Timestamp, jiff::Timestamp),
) -> Result<()> {
    let (from, to) = app
        .screen
        .agenda()
        .map_or(window, |agenda| agenda.loaded_span(window));
    app.appointments_loaded(account, store.appointments_between(from, to)?);
    Ok(())
}

/// Keeps `signature` for `address`, reading the accounts again so a sign-in
/// since tuilook started is not lost.
fn save_signature(path: &Path, address: &str, signature: &str) -> std::io::Result<()> {
    let mut accounts = Accounts::load(path)?;
    accounts.set_signature(address, signature);
    accounts.save(path)
}

/// Every account's cached messages matching `query`, with the account each
/// is in, newest first.
fn search_all(mailboxes: &[Mailbox], query: &str) -> Result<Vec<(usize, Message)>> {
    let mut found = Vec::new();
    for (account, mailbox) in mailboxes.iter().enumerate() {
        let messages = mailbox.store.search(query, SEARCH_LIMIT)?;
        found.extend(messages.into_iter().map(|m| (account, m)));
    }
    found.sort_by_key(|(_, m)| std::cmp::Reverse(m.received_at));
    found.truncate(SEARCH_LIMIT);
    Ok(found)
}

/// The user's downloads folder, or else their home folder, or else the
/// folder tuilook was started in.
fn downloads() -> PathBuf {
    let dirs = directories::UserDirs::new();
    dirs.as_ref()
        .and_then(|d| d.download_dir().map(Path::to_path_buf))
        .or_else(|| dirs.as_ref().map(|d| d.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}
