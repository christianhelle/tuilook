use crate::model::{Attachment, BackendError, FolderId, Invite, MailBackend, MessageId};
use crate::store::{Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Refreshes the folder tree from the backend, forgetting folders that
/// have been deleted there.
pub async fn sync_folders(backend: &impl MailBackend, store: &mut Store) -> Result<(), SyncError> {
    let folders = backend.list_folders().await?;
    store.upsert_folders(&folders)?;
    let ids: Vec<_> = folders.into_iter().map(|f| f.id).collect();
    store.retain_folders(&ids)?;
    Ok(())
}

/// Pulls a folder's message changes from the backend, then its pins, which
/// delta sync does not carry.
pub async fn sync_messages(
    backend: &impl MailBackend,
    store: &mut Store,
    folder: &FolderId,
) -> Result<(), SyncError> {
    let link = store.delta_link(folder)?;
    let page = match backend.delta_sync(folder, link.as_deref()).await {
        Err(BackendError::DeltaExpired) => {
            // Start over and drop whatever the fresh sync no longer has.
            let page = backend.delta_sync(folder, None).await?;
            let synced: Vec<_> = page.upserted.iter().map(|m| m.id.clone()).collect();
            store.retain_messages_in(folder, &synced)?;
            page
        }
        result => result?,
    };
    store.upsert_messages(&page.upserted)?;
    store.change_messages(&page.changed)?;
    store.delete_messages(&page.removed)?;
    store.set_delta_link(folder, page.delta_link.as_deref())?;
    let pinned = backend.pinned_messages(folder).await?;
    store.upsert_messages(&pinned)?;
    let ids: Vec<_> = pinned.into_iter().map(|m| m.id).collect();
    store.set_pinned_in(folder, &ids)?;
    Ok(())
}

/// The text of a message body and the files attached to it, fetched once
/// and then served from the store.
pub async fn load_body(
    backend: &impl MailBackend,
    store: &mut Store,
    message: &MessageId,
) -> Result<(String, Vec<Attachment>), SyncError> {
    let (body, attachments) = match store.body(message)? {
        Some(body) => (body, store.attachments(message)?),
        None => {
            let body = backend.fetch_body(message).await?;
            let attachments = if store.has_attachments(message)? {
                backend.attachments(message).await?
            } else {
                vec![]
            };
            store.put_attachments(message, &attachments)?;
            store.put_body(message, &body)?;
            (body, attachments)
        }
    };
    Ok((crate::core::body_text(&body), attachments))
}

/// The meeting `message` invites to, if it is an invite. A message known
/// not to be one is not asked about again.
pub async fn load_invite(
    backend: &impl MailBackend,
    store: &mut Store,
    message: &MessageId,
) -> Option<Invite> {
    if let Ok(Some(None)) = store.invite(message) {
        return None;
    }
    let invite = backend.invite(message).await.ok()?;
    store.put_invite(message, invite.as_ref()).ok()?;
    invite
}
