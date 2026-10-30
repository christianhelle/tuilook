//! In-memory [`MailBackend`] for tests and the offline demo shell.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use jiff::Timestamp;

use crate::model::{
    Appointment, AppointmentDraft, AppointmentId, BackendError, Body, DeltaPage, Draft, Folder,
    FolderId, MailBackend, Message, MessageId, OlderPage,
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
    /// Messages moved so far, and the folder each went to. Later syncs
    /// report them in that folder.
    pub moved: Arc<Mutex<Vec<(MessageId, FolderId)>>>,
    /// Messages that fail to delete or move.
    pub undeletable: HashSet<MessageId>,
    /// Messages marked read so far. Later syncs report them as read.
    pub read: Arc<Mutex<HashSet<MessageId>>>,
    /// Messages pinned on the server, in `messages` or `older`.
    pub pinned: Arc<Mutex<HashSet<MessageId>>>,
    /// Messages flagged on the server.
    pub flagged: Arc<Mutex<HashSet<MessageId>>>,
    /// Messages sent so far.
    pub sent: Arc<Mutex<Vec<Draft>>>,
    /// Why listing folders fails, if it does.
    pub folders_error: Option<BackendError>,
    /// The calendar, as the server has it.
    pub appointments: Arc<Mutex<Vec<Appointment>>>,
    /// Why every calendar call fails, if it does.
    pub calendar_error: Option<BackendError>,
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
        let moved = self.moved.lock().expect("no test panics holding it");
        let now_in: HashMap<&MessageId, &FolderId> = moved.iter().map(|(m, f)| (m, f)).collect();
        let mut upserted: Vec<Message> = self
            .messages
            .iter()
            .flat_map(|(was_in, messages)| messages.iter().map(move |m| (was_in, m)))
            .filter(|(was_in, m)| now_in.get(&m.id).copied().unwrap_or(was_in) == folder)
            .map(|(_, m)| m.clone())
            .collect();
        for m in &mut upserted {
            m.folder_id = folder.clone();
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

    async fn move_message(&self, message: &MessageId, to: &FolderId) -> Result<(), BackendError> {
        if self.undeletable.contains(message) {
            return Err(BackendError::Other(format!("{} is locked", message.0)));
        }
        let mut moved = self.moved.lock().expect("no test panics holding it");
        moved.push((message.clone(), to.clone()));
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

    async fn pinned_messages(&self, folder: &FolderId) -> Result<Vec<Message>, BackendError> {
        let pinned = self.pinned.lock().expect("no test panics holding it");
        Ok([&self.messages, &self.older]
            .into_iter()
            .filter_map(|messages| messages.get(folder))
            .flatten()
            .filter(|m| pinned.contains(&m.id))
            .map(|m| Message {
                pinned: true,
                ..m.clone()
            })
            .collect())
    }

    async fn set_pinned(&self, message: &MessageId, pinned: bool) -> Result<(), BackendError> {
        let mut pins = self.pinned.lock().expect("no test panics holding it");
        if pinned {
            pins.insert(message.clone());
        } else {
            pins.remove(message);
        }
        Ok(())
    }

    async fn set_flagged(&self, message: &MessageId, flagged: bool) -> Result<(), BackendError> {
        let mut flags = self.flagged.lock().expect("no test panics holding it");
        if flagged {
            flags.insert(message.clone());
        } else {
            flags.remove(message);
        }
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

    async fn appointments(
        &self,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Appointment>, BackendError> {
        if let Some(error) = &self.calendar_error {
            return Err(error.clone());
        }
        let appointments = self.appointments.lock().expect("no test panics holding it");
        let mut overlapping: Vec<Appointment> = appointments
            .iter()
            .filter(|a| a.start < to && a.end > from)
            .cloned()
            .collect();
        overlapping.sort_by_key(|a| a.start);
        Ok(overlapping)
    }

    async fn create_appointment(&self, draft: &AppointmentDraft) -> Result<(), BackendError> {
        if let Some(error) = &self.calendar_error {
            return Err(error.clone());
        }
        let mut appointments = self.appointments.lock().expect("no test panics holding it");
        let id = AppointmentId(format!("new-{}", appointments.len() + 1));
        appointments.push(appointment(id, draft));
        Ok(())
    }
}

/// What the server makes of `draft`, saved by its organizer.
fn appointment(id: AppointmentId, draft: &AppointmentDraft) -> Appointment {
    Appointment {
        id,
        subject: draft.subject.clone(),
        start: draft.start,
        end: draft.end,
        all_day: draft.all_day,
        location: draft.location.clone(),
        organizer: String::new(),
        attendees: draft.attendees.clone(),
        preview: draft.notes.clone(),
        is_organizer: true,
    }
}
