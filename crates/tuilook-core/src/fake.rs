//! In-memory [`MailBackend`] for tests and the offline demo shell.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use tuilook_model::{
    BackendError, Body, DeltaPage, Draft, Folder, FolderId, MailBackend, Message, MessageId,
};

#[derive(Debug, Clone, Default)]
pub struct FakeBackend {
    pub folders: Vec<Folder>,
    pub messages: HashMap<FolderId, Vec<Message>>,
    pub bodies: HashMap<MessageId, Body>,
    /// Answers for a folder's delta sync from a given link. A first sync
    /// (`None`) that is not scripted returns all of the folder's `messages`.
    pub deltas: HashMap<(FolderId, Option<String>), Result<DeltaPage, BackendError>>,
    /// Messages deleted so far, and whether permanently.
    pub deleted: Arc<Mutex<Vec<(MessageId, bool)>>>,
    /// Messages that fail to delete.
    pub undeletable: HashSet<MessageId>,
    /// Messages sent so far.
    pub sent: Arc<Mutex<Vec<Draft>>>,
    /// Why listing folders fails, if it does.
    pub folders_error: Option<BackendError>,
}

impl MailBackend for FakeBackend {
    async fn list_folders(&self) -> Result<Vec<Folder>, BackendError> {
        if let Some(error) = &self.folders_error {
            return Err(error.clone());
        }
        Ok(self.folders.clone())
    }

    async fn delta_sync(
        &self,
        folder: &FolderId,
        delta_link: Option<&str>,
    ) -> Result<DeltaPage, BackendError> {
        let key = (folder.clone(), delta_link.map(str::to_owned));
        if let Some(scripted) = self.deltas.get(&key) {
            return scripted.clone();
        }
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

    async fn delete(&self, message: &MessageId, permanent: bool) -> Result<(), BackendError> {
        if self.undeletable.contains(message) {
            return Err(BackendError::Other(format!("{} is locked", message.0)));
        }
        let mut deleted = self.deleted.lock().expect("no test panics holding it");
        deleted.push((message.clone(), permanent));
        Ok(())
    }

    async fn send(&self, draft: &Draft) -> Result<(), BackendError> {
        if draft.to.iter().any(|to| to.ends_with("@invalid")) {
            return Err(BackendError::Other("no such recipient".to_owned()));
        }
        if draft.to.iter().any(|to| to.ends_with("@blocked")) {
            return Err(BackendError::SendingBlocked("spam suspected".to_owned()));
        }
        let mut sent = self.sent.lock().expect("no test panics holding it");
        sent.push(draft.clone());
        Ok(())
    }
}
