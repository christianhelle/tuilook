//! Changes to mail that is already in a folder: what the server is told, what
//! the store keeps, and what the UI hears, including when it goes wrong.

use tokio::sync::mpsc;

use crate::core::sync_messages;
use crate::model::{BackendError, Event, FolderId, MailBackend, MessageId};
use crate::store::Store;

/// Mail leaving its folder, on the server and in the store.
#[derive(Debug, Clone, Copy)]
pub(super) enum Removal<'a> {
    Delete { permanent: bool },
    // Graph gives a moved message a new id, so it leaves the store as well; a
    // sync of the folder it went to brings it back.
    Move { to: &'a FolderId },
}

impl Removal<'_> {
    async fn make(self, backend: &impl MailBackend, id: &MessageId) -> Result<(), BackendError> {
        match self {
            Removal::Delete { permanent } => backend.delete(id, permanent).await,
            Removal::Move { to } => backend.move_message(id, to).await,
        }
    }

    fn done(self, ids: Vec<MessageId>) -> Event {
        match self {
            Removal::Delete { .. } => Event::MessagesDeleted(ids),
            Removal::Move { to } => Event::MessagesMoved {
                ids,
                to: to.clone(),
            },
        }
    }

    /// Why the mail did not go, when `gone` of it did.
    fn failed(self, gone: usize, failed: usize, first: &str) -> String {
        let (verb, past) = match self {
            Removal::Delete { .. } => ("delete", "deleted"),
            Removal::Move { .. } => ("move", "moved"),
        };
        match gone {
            0 => format!("could not {verb}: {first}"),
            n => format!("{past} {n}, {failed} failed: {first}"),
        }
    }
}

/// Makes `removal` of each message, reporting the ones that went and why any
/// did not. Returns how many went.
pub(super) async fn remove_messages(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    removal: Removal<'_>,
    events: &mpsc::Sender<Event>,
) -> Result<usize, mpsc::error::SendError<Event>> {
    let mut gone = Vec::new();
    let mut failures = Vec::new();
    for id in ids {
        match removal.make(backend, id).await {
            Ok(()) => gone.push(id.clone()),
            Err(e) => failures.push(e.to_string()),
        }
    }
    if let Err(e) = store.delete_messages(&gone) {
        failures.push(e.to_string());
    }
    let count = gone.len();
    if count > 0 {
        events.send(removal.done(gone)).await?;
    }
    if let Some(first) = failures.first() {
        let error = removal.failed(count, failures.len(), first);
        events.send(Event::Error(error)).await?;
    }
    Ok(count)
}

/// Moves each message, then syncs the folder they went to so they show there.
pub(super) async fn move_messages(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    to: FolderId,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let removal = Removal::Move { to: &to };
    if remove_messages(backend, store, ids, removal, events).await? == 0 {
        return Ok(());
    }
    let event = match sync_messages(backend, store, &to).await {
        Ok(()) => Event::FolderSynced(to),
        Err(e) => Event::Error(e.to_string()),
    };
    events.send(event).await
}

/// A change to a message that the UI shows before the server has agreed.
#[derive(Debug, Clone, Copy)]
pub(super) enum Change {
    Read,
    Pinned(bool),
    Flagged(bool),
}

impl Change {
    /// What could not be done, for the error.
    fn verb(self) -> &'static str {
        match self {
            Change::Read => "mark as read",
            Change::Pinned(true) => "pin",
            Change::Pinned(false) => "unpin",
            Change::Flagged(true) => "flag",
            Change::Flagged(false) => "unflag",
        }
    }

    /// Makes the change on the server, then in the store.
    async fn make(
        self,
        backend: &impl MailBackend,
        store: &mut Store,
        id: &MessageId,
    ) -> Result<(), String> {
        let told = match self {
            Change::Read => backend.mark_read(id).await,
            Change::Pinned(pinned) => backend.set_pinned(id, pinned).await,
            Change::Flagged(flagged) => backend.set_flagged(id, flagged).await,
        };
        told.map_err(|e| e.to_string())?;
        let stored = match self {
            Change::Read => store.mark_read(id),
            Change::Pinned(pinned) => store.set_pinned(id, pinned),
            Change::Flagged(flagged) => store.set_flagged(id, flagged),
        };
        stored.map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Makes `change` to each message, reporting why any failed, then reports
/// each folder they are in. Failed ones count too: the UI showed the change
/// already, and reading the folder again takes it back.
pub(super) async fn change_messages(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    change: Change,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let mut changed = Vec::new();
    for id in ids {
        if let Err(e) = change.make(backend, store, id).await {
            let verb = change.verb();
            events
                .send(Event::Error(format!("could not {verb}: {e}")))
                .await?;
        }
        if let Ok(Some(folder)) = store.folder_of(id)
            && !changed.contains(&folder)
        {
            changed.push(folder);
        }
    }
    for folder in changed {
        events.send(Event::FolderSynced(folder)).await?;
    }
    Ok(())
}
