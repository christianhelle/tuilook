//! Saving attached files to disk.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::model::{Attachment, Event, MailBackend, MessageId};

/// Saves each of `attachments` of `message` into `dir`, reporting where
/// each went, or why it did not.
pub async fn save_attachments(
    backend: &impl MailBackend,
    message: &MessageId,
    attachments: &[Attachment],
    dir: &Path,
    events: &mpsc::Sender<Event>,
) -> Result<(), mpsc::error::SendError<Event>> {
    for attachment in attachments {
        let event = match save(backend, message, attachment, dir).await {
            Ok(path) => Event::AttachmentSaved(path),
            Err(e) => Event::Error(format!("could not save {}: {e}", attachment.name)),
        };
        events.send(event).await?;
    }
    Ok(())
}

async fn save(
    backend: &impl MailBackend,
    message: &MessageId,
    attachment: &Attachment,
    dir: &Path,
) -> Result<PathBuf, String> {
    let content = backend
        .attachment_content(message, &attachment.id)
        .await
        .map_err(|e| e.to_string())?;
    let path = free_path(dir, &attachment.name);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

/// `name` in `dir`, numbered like `report (1).pdf` if a file has it already.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let name = Path::new(name);
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
    let extension = name
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|path| !path.exists())
        .expect("some number is free")
}
