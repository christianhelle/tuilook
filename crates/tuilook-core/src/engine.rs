use tokio::sync::mpsc;
use tuilook_model::{Draft, Event, MailBackend, MessageId, WellKnown};
use tuilook_store::Store;

use crate::{load_body, sync_folders, sync_messages};

/// What the UI asks core to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Refresh the folder tree, then every folder's messages.
    SyncAll,
    LoadBody(MessageId),
    /// Delete messages: to Deleted Items, or for good when `permanent`.
    Delete {
        ids: Vec<MessageId>,
        permanent: bool,
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
            Command::SyncAll => sync_all(&backend, &mut store, &events).await,
            Command::LoadBody(message) => {
                let event = match load_body(&backend, &mut store, &message).await {
                    Ok(text) => Event::BodyLoaded { message, text },
                    Err(e) => Event::Error(e.to_string()),
                };
                events.send(event).await
            }
            Command::Delete { ids, permanent } => {
                delete(&backend, &mut store, &ids, permanent, &events).await
            }
            Command::Send(draft) => send(&backend, &mut store, &draft, &events).await,
        };
        if sent.is_err() {
            return; // The UI has gone away.
        }
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

/// Sends a message, then syncs Sent Items so it shows there.
async fn send(
    backend: &impl MailBackend,
    store: &mut Store,
    draft: &Draft,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    if let Err(e) = backend.send(draft).await {
        return events.send(Event::SendFailed(e.to_string())).await;
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
