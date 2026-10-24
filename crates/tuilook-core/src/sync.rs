use tuilook_model::{BackendError, FolderId, MailBackend};
use tuilook_store::{Store, StoreError};

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
