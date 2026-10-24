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
    let page = backend.delta_sync(folder, None).await?;
    store.upsert_messages(&page.upserted)?;
    Ok(())
}
