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
use crate::core::Command;
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
    pub commands: mpsc::Sender<Command>,
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
            Event::BodyLoaded { message, text } => app.body_loaded(message, text),
            Event::MessagesDeleted(ids) => app.messages_deleted(&ids),
            Event::MessagesMoved { ids, to } => app.messages_moved(&ids, &to),
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
            // Nothing shows the calendar yet.
            Event::CalendarSynced | Event::AppointmentSaved | Event::AppointmentDeleted(_) => None,
            Event::Error(error) => {
                match app.accounts.get(account).filter(|_| app.accounts.len() > 1) {
                    Some(address) => app.show_error(format!("{address}: {error}")),
                    None => app.show_error(error),
                }
                None
            }
        })
    }
}

/// Shows `folder`'s messages from the store again, if it is the one shown.
fn refresh_shown(app: &mut AppState, store: &Store, folder: &FolderId) -> Result<Option<Effect>> {
    if app.results_query().is_some() || app.shown_folder().as_ref() != Some(folder) {
        return Ok(None);
    }
    let messages = if app.shows_unread() {
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
                Effect::LoadFolder(id) => app.set_messages(store.messages_in(&id)?),
                Effect::LoadUnread(id) => app.set_messages(store.unread_in(&id, &[])?),
                Effect::LoadBody(id) => {
                    commands.blocking_send(Command::LoadBody(id))?;
                    None
                }
                Effect::LoadOlder {
                    folder,
                    before,
                    unread_only,
                } => {
                    let older = Command::LoadOlder {
                        folder,
                        before,
                        unread_only,
                    };
                    commands.blocking_send(older)?;
                    None
                }
                Effect::DeleteFolder { folder, permanent } => {
                    commands.blocking_send(Command::DeleteFolder { folder, permanent })?;
                    None
                }
                Effect::CreateFolder { parent, name, .. } => {
                    commands.blocking_send(Command::CreateFolder { parent, name })?;
                    None
                }
                Effect::RenameFolder { folder, name, .. } => {
                    commands.blocking_send(Command::RenameFolder { folder, name })?;
                    None
                }
                Effect::Move { ids, to, .. } => {
                    commands.blocking_send(Command::Move { ids, to })?;
                    None
                }
                Effect::MarkRead(id) => {
                    commands.blocking_send(Command::MarkRead(id))?;
                    None
                }
                Effect::Sync => {
                    for mailbox in mailboxes {
                        mailbox.commands.blocking_send(Command::SyncAll)?;
                    }
                    None
                }
                Effect::Send(draft) => {
                    commands.blocking_send(Command::Send(draft))?;
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
                Effect::SetPinned { ids, pinned } => {
                    // Search results may be in several accounts.
                    for (account, ids) in app.by_account(ids) {
                        let pin = Command::SetPinned { ids, pinned };
                        mailboxes[account].commands.blocking_send(pin)?;
                    }
                    None
                }
                Effect::SetFlagged { ids, flagged } => {
                    // Search results may be in several accounts.
                    for (account, ids) in app.by_account(ids) {
                        let flag = Command::SetFlagged { ids, flagged };
                        mailboxes[account].commands.blocking_send(flag)?;
                    }
                    None
                }
                Effect::Delete { ids, permanent } => {
                    // Search results may be in several accounts.
                    for (account, ids) in app.by_account(ids) {
                        let delete = Command::Delete { ids, permanent };
                        mailboxes[account].commands.blocking_send(delete)?;
                    }
                    None
                }
            };
        }
        Ok(())
    }
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
