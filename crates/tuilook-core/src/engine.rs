use tokio::sync::mpsc;
use tuilook_model::{Event, MailBackend, MessageId};
use tuilook_store::Store;

use crate::{load_body, sync_folders, sync_messages};

/// What the UI asks core to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Refresh the folder tree, then every folder's messages.
    SyncAll,
    LoadBody(MessageId),
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
        };
        if sent.is_err() {
            return; // The UI has gone away.
        }
    }
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
