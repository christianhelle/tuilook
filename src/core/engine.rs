use std::collections::HashSet;

use crate::model::{BackendError, Draft, Event, FolderId, MailBackend, MessageId, WellKnown};
use crate::store::Store;
use jiff::Timestamp;
use tokio::sync::mpsc;

use crate::core::{load_body, sync_folders, sync_messages};

/// What the UI asks core to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Refresh the folder tree, then every folder's messages.
    SyncAll,
    LoadBody(MessageId),
    /// Fetch a page of a folder's mail received at or before `before`, which
    /// sync does not reach, and only unread mail when `unread_only`.
    LoadOlder {
        folder: FolderId,
        before: Timestamp,
        unread_only: bool,
    },
    /// Delete a folder: to Deleted Items, or for good when `permanent`.
    DeleteFolder {
        folder: FolderId,
        permanent: bool,
    },
    /// Make a folder called `name` inside `parent`, or at the top level.
    CreateFolder {
        parent: Option<FolderId>,
        name: String,
    },
    /// Give a folder a new name.
    RenameFolder {
        folder: FolderId,
        name: String,
    },
    /// Delete messages: to Deleted Items, or for good when `permanent`.
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
    },
    /// Move messages into another folder.
    Move {
        ids: Vec<MessageId>,
        to: FolderId,
    },
    /// Mark a message read, as it is being shown.
    MarkRead(MessageId),
    /// Pin messages to the top of their folders, or unpin them.
    SetPinned {
        ids: Vec<MessageId>,
        pinned: bool,
    },
    Send(Draft),
}

/// Runs until the UI drops its command sender. Owns every write to `store`.
pub async fn run_engine(
    backend: impl MailBackend,
    mut store: Store,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
) {
    while let Some(command) = commands.recv().await {
        let sent = match command {
            Command::SyncAll => match sync_all(&backend, &mut store, &events).await {
                Ok(()) => events.send(Event::SyncFinished).await,
                gone => gone,
            },
            Command::LoadBody(message) => {
                let event = match load_body(&backend, &mut store, &message).await {
                    Ok(text) => Event::BodyLoaded { message, text },
                    Err(e) => Event::Error(e.to_string()),
                };
                events.send(event).await
            }
            Command::LoadOlder {
                folder,
                before,
                unread_only,
            } => load_older(&backend, &mut store, folder, before, unread_only, &events).await,
            Command::DeleteFolder { folder, permanent } => {
                delete_folder(&backend, &mut store, folder, permanent, &events).await
            }
            Command::CreateFolder { parent, name } => {
                let created = backend.create_folder(parent.as_ref(), &name).await;
                let done = created
                    .map(|()| Event::FolderCreated(name))
                    .map_err(|e| format!("could not create the folder: {e}"));
                folder_changed(&backend, &mut store, done, &events).await
            }
            Command::RenameFolder { folder, name } => {
                let renamed = backend.rename_folder(&folder, &name).await;
                let done = renamed
                    .map(|()| Event::FolderRenamed { folder, name })
                    .map_err(|e| format!("could not rename the folder: {e}"));
                folder_changed(&backend, &mut store, done, &events).await
            }
            Command::Delete { ids, permanent } => {
                delete(&backend, &mut store, &ids, permanent, &events).await
            }
            Command::Move { ids, to } => {
                move_messages(&backend, &mut store, &ids, to, &events).await
            }
            Command::MarkRead(message) => {
                let event = match mark_read(&backend, &mut store, &message).await {
                    Ok(Some(folder)) => Some(Event::FolderSynced(folder)),
                    Ok(None) => None,
                    Err(e) => Some(Event::Error(format!("could not mark as read: {e}"))),
                };
                match event {
                    Some(event) => events.send(event).await,
                    None => Ok(()),
                }
            }
            Command::SetPinned { ids, pinned } => {
                set_pinned(&backend, &mut store, &ids, pinned, &events).await
            }
            Command::Send(draft) => send(&backend, &mut store, &draft, &events).await,
        };
        if sent.is_err() {
            return; // The UI has gone away.
        }
    }
}

/// Caches a page of a folder's older mail. There is more to fetch only
/// while pages bring mail not cached yet, so a page of mail already cached
/// cannot have the UI ask for the same page forever.
async fn load_older(
    backend: &impl MailBackend,
    store: &mut Store,
    folder: FolderId,
    before: Timestamp,
    unread_only: bool,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let fetched = backend.older_messages(&folder, before, unread_only).await;
    let cached = store
        .messages_in(&folder)
        .map(|messages| messages.into_iter().map(|m| m.id).collect::<HashSet<_>>());
    let more = match (fetched, cached) {
        (Ok(page), Ok(cached)) => {
            let new = page.messages.iter().any(|m| !cached.contains(&m.id));
            match store.upsert_messages(&page.messages) {
                Ok(()) => page.more && new,
                Err(e) => {
                    events.send(Event::Error(e.to_string())).await?;
                    false
                }
            }
        }
        (Err(e), _) => {
            let error = format!("could not load older mail: {e}");
            events.send(Event::Error(error)).await?;
            false
        }
        (_, Err(e)) => {
            events.send(Event::Error(e.to_string())).await?;
            false
        }
    };
    events
        .send(Event::OlderLoaded {
            folder,
            unread_only,
            more,
        })
        .await
}

/// Deletes a folder, then refreshes the folder tree so it leaves it, or
/// shows up again under Deleted Items.
async fn delete_folder(
    backend: &impl MailBackend,
    store: &mut Store,
    folder: FolderId,
    permanent: bool,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    if let Err(e) = backend.delete_folder(&folder, permanent).await {
        let error = format!("could not delete the folder: {e}");
        return events.send(Event::Error(error)).await;
    }
    events.send(Event::FolderDeleted(folder)).await?;
    match sync_folders(backend, store).await {
        Ok(()) => events.send(Event::FoldersSynced).await,
        Err(e) => events.send(Event::Error(e.to_string())).await,
    }
}

/// Reports a change to the folder tree, or why it failed, then refreshes
/// the tree so the change shows.
async fn folder_changed(
    backend: &impl MailBackend,
    store: &mut Store,
    done: Result<Event, String>,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    match done {
        Ok(event) => events.send(event).await?,
        Err(error) => return events.send(Event::Error(error)).await,
    }
    match sync_folders(backend, store).await {
        Ok(()) => events.send(Event::FoldersSynced).await,
        Err(e) => events.send(Event::Error(e.to_string())).await,
    }
}

/// Deletes each message, reporting the ones that went and why any did not.
async fn delete(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    permanent: bool,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let mut deleted = Vec::new();
    let mut failures = Vec::new();
    for id in ids {
        match backend.delete(id, permanent).await {
            Ok(()) => deleted.push(id.clone()),
            Err(e) => failures.push(e.to_string()),
        }
    }
    if let Err(e) = store.delete_messages(&deleted) {
        failures.push(e.to_string());
    }
    if !deleted.is_empty() {
        events.send(Event::MessagesDeleted(deleted.clone())).await?;
    }
    if let Some(first) = failures.first() {
        let error = match deleted.len() {
            0 => format!("could not delete: {first}"),
            n => format!("deleted {n}, {} failed: {first}", failures.len()),
        };
        events.send(Event::Error(error)).await?;
    }
    Ok(())
}

/// Moves each message, reporting the ones that went and why any did not,
/// then syncs the folder they went to so they show there.
async fn move_messages(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    to: FolderId,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let mut moved = Vec::new();
    let mut failures = Vec::new();
    for id in ids {
        match backend.move_message(id, &to).await {
            Ok(()) => moved.push(id.clone()),
            Err(e) => failures.push(e.to_string()),
        }
    }
    // Graph gives a moved message a new id; the sync below brings it back.
    if let Err(e) = store.delete_messages(&moved) {
        failures.push(e.to_string());
    }
    if !moved.is_empty() {
        let event = Event::MessagesMoved {
            ids: moved.clone(),
            to: to.clone(),
        };
        events.send(event).await?;
    }
    if let Some(first) = failures.first() {
        let error = match moved.len() {
            0 => format!("could not move: {first}"),
            n => format!("moved {n}, {} failed: {first}", failures.len()),
        };
        events.send(Event::Error(error)).await?;
    }
    if moved.is_empty() {
        return Ok(());
    }
    let event = match sync_messages(backend, store, &to).await {
        Ok(()) => Event::FolderSynced(to),
        Err(e) => Event::Error(e.to_string()),
    };
    events.send(event).await
}

/// Marks a message read on the server, then in the store. Returns the folder
/// whose messages and unread count changed, if one did.
async fn mark_read(
    backend: &impl MailBackend,
    store: &mut Store,
    message: &MessageId,
) -> Result<Option<FolderId>, String> {
    backend
        .mark_read(message)
        .await
        .map_err(|e| e.to_string())?;
    store.mark_read(message).map_err(|e| e.to_string())
}

/// Pins or unpins messages on the server, then in the store, and reports
/// each folder whose messages changed.
async fn set_pinned(
    backend: &impl MailBackend,
    store: &mut Store,
    ids: &[MessageId],
    pinned: bool,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    let mut changed = Vec::new();
    for id in ids {
        let done = match backend.set_pinned(id, pinned).await {
            Ok(()) => store.set_pinned(id, pinned).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match done {
            Ok(Some(folder)) if !changed.contains(&folder) => changed.push(folder),
            Ok(_) => {}
            Err(e) => {
                let verb = if pinned { "pin" } else { "unpin" };
                events
                    .send(Event::Error(format!("could not {verb}: {e}")))
                    .await?;
            }
        }
    }
    for folder in changed {
        events.send(Event::FolderSynced(folder)).await?;
    }
    Ok(())
}

/// Sends a message, then syncs Sent Items so it shows there.
async fn send(
    backend: &impl MailBackend,
    store: &mut Store,
    draft: &Draft,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    match backend.send(draft).await {
        Ok(()) => {}
        Err(BackendError::SendingBlocked(said)) => {
            return events.send(Event::SendBlocked(said)).await;
        }
        Err(e) => return events.send(Event::SendFailed(e.to_string())).await,
    }
    events.send(Event::Sent).await?;
    let sent_items = match store.folders() {
        Ok(folders) => folders
            .into_iter()
            .find(|f| f.well_known == Some(WellKnown::SentItems)),
        Err(e) => return events.send(Event::Error(e.to_string())).await,
    };
    if let Some(folder) = sent_items {
        let event = match sync_messages(backend, store, &folder.id).await {
            Ok(()) => Event::FolderSynced(folder.id),
            Err(e) => Event::Error(e.to_string()),
        };
        events.send(event).await?;
    }
    Ok(())
}

async fn sync_all(
    backend: &impl MailBackend,
    store: &mut Store,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    if let Err(e) = sync_folders(backend, store).await {
        return events.send(Event::Error(e.to_string())).await;
    }
    events.send(Event::FoldersSynced).await?;
    let folders = match store.folders() {
        Ok(folders) => folders,
        Err(e) => return events.send(Event::Error(e.to_string())).await,
    };
    for folder in folders {
        let event = match sync_messages(backend, store, &folder.id).await {
            Ok(()) => Event::FolderSynced(folder.id),
            Err(e) => Event::Error(e.to_string()),
        };
        events.send(event).await?;
    }
    Ok(())
}
