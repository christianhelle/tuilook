//! Saving attached files to disk.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::model::{Attachment, AttachmentKind, Event, MailBackend, MessageId};

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
        if attachment.kind == AttachmentKind::Reference {
            let skipped = format!(
                "{} is a link to a file in the cloud, with nothing to save",
                attachment.name
            );
            events.send(Event::Error(skipped)).await?;
            continue;
        }
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
    let path = free_path(dir, &file_name(attachment));
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A name to save `attachment` as that keeps it in the folder it is saved
/// to and that every system allows. Attached mail is saved as MIME.
fn file_name(attachment: &Attachment) -> String {
    let name: String = attachment
        .name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows drops dots and spaces at the end of a name.
    let name = name.trim_end_matches(['.', ' ']);
    let name = if name.is_empty() { "attachment" } else { name };
    let eml = name.to_lowercase().ends_with(".eml");
    if attachment.kind == AttachmentKind::Item && !eml {
        format!("{name}.eml")
    } else {
        name.to_owned()
    }
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
