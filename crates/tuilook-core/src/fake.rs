//! In-memory [`MailBackend`] for tests and the offline demo shell.

use std::collections::HashMap;

use tuilook_model::{
    BackendError, Body, DeltaPage, Folder, FolderId, MailBackend, Message, MessageId,
};

#[derive(Debug, Clone, Default)]
pub struct FakeBackend {
    pub folders: Vec<Folder>,
    pub messages: HashMap<FolderId, Vec<Message>>,
    pub bodies: HashMap<MessageId, Body>,
}

impl MailBackend for FakeBackend {
    async fn list_folders(&self) -> Result<Vec<Folder>, BackendError> {
        Ok(self.folders.clone())
    }

    async fn delta_sync(
        &self,
        folder: &FolderId,
        _delta_link: Option<&str>,
    ) -> Result<DeltaPage, BackendError> {
        Ok(DeltaPage {
            upserted: self.messages.get(folder).cloned().unwrap_or_default(),
            ..Default::default()
        })
    }

    async fn fetch_body(&self, message: &MessageId) -> Result<Body, BackendError> {
        self.bodies
            .get(message)
            .cloned()
            .ok_or_else(|| BackendError::Other(format!("no body for {}", message.0)))
    }
}
