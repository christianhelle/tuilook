//! In-memory [`MailBackend`] for tests and the offline demo shell.

use std::collections::HashMap;

use tuilook_model::{BackendError, DeltaPage, Folder, FolderId, MailBackend, Message};

#[derive(Debug, Clone, Default)]
pub struct FakeBackend {
    pub folders: Vec<Folder>,
    pub messages: HashMap<FolderId, Vec<Message>>,
}

impl MailBackend for FakeBackend {
    async fn list_folders(&self) -> Result<Vec<Folder>, BackendError> {
        Ok(self.folders.clone())
    }

    async fn delta_sync(
        &self,
        _folder: &FolderId,
        _delta_link: Option<&str>,
    ) -> Result<DeltaPage, BackendError> {
        todo!()
    }
}
