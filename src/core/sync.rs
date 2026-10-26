use crate::model::{BackendError, FolderId, MailBackend, MessageId};
use crate::store::{Store, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Refreshes the folder tree from the backend.
pub async fn sync_folders(backend: &impl MailBackend, store: &mut Store) -> Result<(), SyncError> {
    let folders = backend.list_folders().await?;
    store.upsert_folders(&folders)?;
    Ok(())
}

/// Pulls a folder's message changes from the backend.
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
    store.delete_messages(&page.removed)?;
    store.set_delta_link(folder, page.delta_link.as_deref())?;
    Ok(())
}

/// The text of a message body, fetched once and then served from the store.
pub async fn load_body(
    backend: &impl MailBackend,
    store: &mut Store,
    message: &MessageId,
) -> Result<String, SyncError> {
    let body = match store.body(message)? {
        Some(body) => body,
        None => {
            let body = backend.fetch_body(message).await?;
            store.put_body(message, &body)?;
            body
        }
    };
    Ok(crate::core::body_text(&body))
}
