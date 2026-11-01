use std::{collections::HashSet, path::PathBuf};

use crate::model::{
    AppointmentDraft, AppointmentId, Attachment, BackendError, Draft, Event, FolderId, MailBackend,
    MessageId, WellKnown,
};
use crate::store::Store;
use jiff::Timestamp;
use tokio::sync::mpsc;

use crate::core::{
    attachments::save_attachments,
    calendar::sync_calendar,
    changes::{Change, Removal, change_messages, move_messages, remove_messages},
    load_body, sync_folders, sync_messages,
};

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
    /// Flag messages for follow-up, or clear their flags.
    SetFlagged {
        ids: Vec<MessageId>,
        flagged: bool,
    },
    /// Save `attachments` of `message` as files in `dir`.
    SaveAttachments {
        message: MessageId,
        attachments: Vec<Attachment>,
        dir: PathBuf,
    },
    Send(Draft),
    /// Refresh the appointments around today.
    SyncCalendar,
    CreateAppointment(AppointmentDraft),
    /// Change an appointment to be as `draft` has it.
    UpdateAppointment {
        id: AppointmentId,
        draft: AppointmentDraft,
    },
    DeleteAppointment(AppointmentId),
}

/// Runs until the UI drops its command sender. Owns every write to `store`.
pub async fn run_engine(
    backend: impl MailBackend,
    mut store: Store,
    mut commands: mpsc::UnboundedReceiver<Command>,
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
                    Ok((text, attachments)) => Event::BodyLoaded {
                        message,
                        text,
                        attachments,
                    },
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
                let removal = Removal::Delete { permanent };
                remove_messages(&backend, &mut store, &ids, removal, &events)
                    .await
                    .map(|_| ())
            }
            Command::Move { ids, to } => {
                move_messages(&backend, &mut store, &ids, to, &events).await
            }
            Command::MarkRead(message) => {
                let ids = [message];
                change_messages(&backend, &mut store, &ids, Change::Read, &events).await
            }
            Command::SetPinned { ids, pinned } => {
                let change = Change::Pinned(pinned);
                change_messages(&backend, &mut store, &ids, change, &events).await
            }
            Command::SetFlagged { ids, flagged } => {
                let change = Change::Flagged(flagged);
                change_messages(&backend, &mut store, &ids, change, &events).await
            }
            Command::SaveAttachments {
                message,
                attachments,
                dir,
            } => save_attachments(&backend, &message, &attachments, &dir, &events).await,
            Command::Send(draft) => send(&backend, &mut store, &draft, &events).await,
            Command::SyncCalendar => {
                let event = match sync_calendar(&backend, &mut store).await {
                    Ok(()) => Event::CalendarSynced,
                    Err(e) => Event::Error(format!("could not sync the calendar: {e}")),
                };
                events.send(event).await
            }
            Command::CreateAppointment(draft) => {
                let saved = backend.create_appointment(&draft).await;
                appointment_saved(&backend, &mut store, saved, &events).await
            }
            Command::UpdateAppointment { id, draft } => {
                let saved = backend.update_appointment(&id, &draft).await;
                appointment_saved(&backend, &mut store, saved, &events).await
            }
            Command::DeleteAppointment(id) => {
                let deleted = match backend.delete_appointment(&id).await {
                    Ok(()) => store.remove_appointment(&id).map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                let event = match deleted {
                    Ok(()) => Event::AppointmentDeleted(id),
                    Err(e) => Event::Error(format!("could not delete the appointment: {e}")),
                };
                events.send(event).await
            }
        };
        if sent.is_err() {
            return; // The UI has gone away.
        }
    }
}

/// Reports a saved appointment, or why it was not, then syncs the calendar
/// so the change shows.
async fn appointment_saved(
    backend: &impl MailBackend,
    store: &mut Store,
    saved: Result<(), BackendError>,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    if let Err(e) = saved {
        return events.send(Event::AppointmentNotSaved(e.to_string())).await;
    }
    events.send(Event::AppointmentSaved).await?;
    let event = match sync_calendar(backend, store).await {
        Ok(()) => Event::CalendarSynced,
        Err(e) => Event::Error(format!("could not sync the calendar: {e}")),
    };
    events.send(event).await
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
