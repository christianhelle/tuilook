//! In-memory [`MailBackend`] for tests and the offline demo shell.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use jiff::Timestamp;

use crate::model::{
    BackendError, Body, DeltaPage, Draft, Folder, FolderId, MailBackend, Message, MessageId,
    OlderPage,
};

/// Older messages the fake hands out at a time.
pub const OLDER_PAGE: usize = 2;

#[derive(Debug, Clone, Default)]
pub struct FakeBackend {
    pub folders: Vec<Folder>,
    pub messages: HashMap<FolderId, Vec<Message>>,
    pub bodies: HashMap<MessageId, Body>,
    /// Answers for a folder's delta sync from a given link. A first sync
    /// (`None`) that is not scripted returns all of the folder's `messages`.
    pub deltas: HashMap<(FolderId, Option<String>), Result<DeltaPage, BackendError>>,
    /// Each folder's mail from before what delta sync reaches, served a
    /// page of [`OLDER_PAGE`] at a time.
    pub older: HashMap<FolderId, Vec<Message>>,
    /// Folders created so far, listed after `folders`.
    pub created: Arc<Mutex<Vec<Folder>>>,
    /// New names given to folders so far.
    pub renamed: Arc<Mutex<HashMap<FolderId, String>>>,
    /// Folders deleted so far, and whether permanently.
    pub deleted_folders: Arc<Mutex<Vec<(FolderId, bool)>>>,
    /// Messages deleted so far, and whether permanently.
    pub deleted: Arc<Mutex<Vec<(MessageId, bool)>>>,
    /// Messages that fail to delete.
    pub undeletable: HashSet<MessageId>,
    /// Messages marked read so far. Later syncs report them as read.
    pub read: Arc<Mutex<HashSet<MessageId>>>,
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
        let created = self.created.lock().expect("no test panics holding it");
        let renamed = self.renamed.lock().expect("no test panics holding it");
        let mut folders: Vec<Folder> = self.folders.iter().chain(created.iter()).cloned().collect();
        for folder in &mut folders {
            if let Some(name) = renamed.get(&folder.id) {
                folder.name.clone_from(name);
            }
        }
        Ok(folders)
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
        let read = self.read.lock().expect("no test panics holding it");
        let mut upserted = self.messages.get(folder).cloned().unwrap_or_default();
        for m in &mut upserted {
            m.is_read |= read.contains(&m.id);
        }
        Ok(DeltaPage {
            upserted,
            ..Default::default()
        })
    }

    async fn older_messages(
        &self,
        folder: &FolderId,
        before: Timestamp,
        unread_only: bool,
    ) -> Result<OlderPage, BackendError> {
        let mut older: Vec<Message> = self
            .older
            .get(folder)
            .into_iter()
            .flatten()
            .filter(|m| m.received_at <= before && !(unread_only && m.is_read))
            .cloned()
            .collect();
        older.sort_by_key(|m| std::cmp::Reverse(m.received_at));
        let more = older.len() > OLDER_PAGE;
        older.truncate(OLDER_PAGE);
        Ok(OlderPage {
            messages: older,
            more,
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

    async fn delete_folder(&self, folder: &FolderId, permanent: bool) -> Result<(), BackendError> {
        let mut deleted = self
            .deleted_folders
            .lock()
            .expect("no test panics holding it");
        deleted.push((folder.clone(), permanent));
        Ok(())
    }

    async fn create_folder(
        &self,
        parent: Option<&FolderId>,
        name: &str,
    ) -> Result<(), BackendError> {
        let mut created = self.created.lock().expect("no test panics holding it");
        let taken = self
            .folders
            .iter()
            .chain(created.iter())
            .any(|f| f.parent_id.as_ref() == parent && f.name.eq_ignore_ascii_case(name));
        if taken {
            return Err(BackendError::Other(format!("{name} already exists")));
        }
        let id = match parent {
            Some(parent) => format!("{}/{name}", parent.0),
            None => name.to_owned(),
        };
        created.push(Folder {
            id: FolderId(id),
            parent_id: parent.cloned(),
            name: name.to_owned(),
            well_known: None,
            unread: 0,
            total: 0,
        });
        Ok(())
    }

    async fn rename_folder(&self, folder: &FolderId, name: &str) -> Result<(), BackendError> {
        let mut renamed = self.renamed.lock().expect("no test panics holding it");
        renamed.insert(folder.clone(), name.to_owned());
        Ok(())
    }

    async fn mark_read(&self, message: &MessageId) -> Result<(), BackendError> {
        let mut read = self.read.lock().expect("no test panics holding it");
        read.insert(message.clone());
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
