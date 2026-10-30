//! Local SQLite cache. The UI reads from it; only core writes to it.

use std::{collections::HashSet, sync::LazyLock};

use crate::model::{
    Body, BodyType, Folder, FolderId, Importance, Message, MessageChange, MessageId, WellKnown,
    sort_folders,
};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Row, types::Type};
use rusqlite_migration::{M, Migrations};

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_folders_messages.sql")),
        M::up(include_str!("migrations/002_bodies.sql")),
        M::up(include_str!("migrations/003_resync_blanked_messages.sql")),
        M::up(include_str!("migrations/004_pinned.sql")),
    ])
});

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) the cache at `path`. Several handles may
    /// share one file: the UI reads while core writes.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL lets readers proceed while the sync engine writes.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn upsert_folders(&mut self, folders: &[Folder]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO folders (id, parent_id, name, well_known, unread, total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (id) DO UPDATE SET
                     parent_id = excluded.parent_id,
                     name = excluded.name,
                     well_known = excluded.well_known,
                     unread = excluded.unread,
                     total = excluded.total",
            )?;
            for f in folders {
                stmt.execute((
                    &f.id.0,
                    f.parent_id.as_ref().map(|p| &p.0),
                    &f.name,
                    f.well_known.map(WellKnown::as_graph_name),
                    f.unread,
                    f.total,
                ))?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes every folder not in `keep`, and the messages in them.
    pub fn retain_folders(&mut self, keep: &[FolderId]) -> Result<()> {
        let keep: HashSet<&str> = keep.iter().map(|id| id.0.as_str()).collect();
        let tx = self.conn.transaction()?;
        {
            let stale: Vec<String> = tx
                .prepare("SELECT id FROM folders")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|id| !keep.contains(id.as_str()))
                .collect();
            let mut stmt = tx.prepare("DELETE FROM folders WHERE id = ?1")?;
            for id in stale {
                stmt.execute([id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Caches messages. A message keeps its pin when it comes again
    /// unpinned, since only the pin sync knows which mail is pinned.
    pub fn upsert_messages(&mut self, messages: &[Message]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO messages (
                     id, folder_id, conversation_id, subject, from_name, from_addr,
                     received_at, is_read, flagged, importance, has_attachments, preview,
                     pinned)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT (id) DO UPDATE SET
                     folder_id = excluded.folder_id,
                     conversation_id = excluded.conversation_id,
                     subject = excluded.subject,
                     from_name = excluded.from_name,
                     from_addr = excluded.from_addr,
                     received_at = excluded.received_at,
                     is_read = excluded.is_read,
                     flagged = excluded.flagged,
                     importance = excluded.importance,
                     has_attachments = excluded.has_attachments,
                     preview = excluded.preview,
                     pinned = MAX(pinned, excluded.pinned)",
            )?;
            for m in messages {
                stmt.execute((
                    &m.id.0,
                    &m.folder_id.0,
                    &m.conversation_id,
                    &m.subject,
                    &m.from_name,
                    &m.from_addr,
                    m.received_at.as_millisecond(),
                    m.is_read,
                    m.flagged,
                    importance_to_str(m.importance),
                    m.has_attachments,
                    &m.preview,
                    m.pinned,
                ))?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Applies changes to cached messages, leaving the fields a change does
    /// not carry as they are. A change to a message not cached is dropped.
    pub fn change_messages(&mut self, changes: &[MessageChange]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE messages SET
                     conversation_id = COALESCE(?2, conversation_id),
                     subject = COALESCE(?3, subject),
                     from_name = COALESCE(?4, from_name),
                     from_addr = COALESCE(?5, from_addr),
                     is_read = COALESCE(?6, is_read),
                     flagged = COALESCE(?7, flagged),
                     importance = COALESCE(?8, importance),
                     has_attachments = COALESCE(?9, has_attachments),
                     preview = COALESCE(?10, preview)
                 WHERE id = ?1",
            )?;
            for c in changes {
                stmt.execute((
                    &c.id.0,
                    &c.conversation_id,
                    &c.subject,
                    c.from.as_ref().map(|(name, _)| name),
                    c.from.as_ref().map(|(_, addr)| addr),
                    c.is_read,
                    c.flagged,
                    c.importance.map(importance_to_str),
                    c.has_attachments,
                    &c.preview,
                ))?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Marks a message read, counting it out of its folder's unread mail.
    /// Returns the folder when the message was there and unread.
    pub fn mark_read(&mut self, id: &MessageId) -> Result<Option<FolderId>> {
        let tx = self.conn.transaction()?;
        let folder = tx
            .query_row(
                "UPDATE messages SET is_read = 1 WHERE id = ?1 AND is_read = 0
                 RETURNING folder_id",
                [&id.0],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(folder) = &folder {
            tx.execute(
                "UPDATE folders SET unread = unread - 1 WHERE id = ?1 AND unread > 0",
                [folder],
            )?;
        }
        tx.commit()?;
        Ok(folder.map(FolderId))
    }

    pub fn delete_messages(&mut self, ids: &[MessageId]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM messages WHERE id = ?1")?;
            for id in ids {
                stmt.execute([&id.0])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes every message in `folder` that is not in `keep`.
    pub fn retain_messages_in(&mut self, folder: &FolderId, keep: &[MessageId]) -> Result<()> {
        let keep: HashSet<&str> = keep.iter().map(|id| id.0.as_str()).collect();
        let stale: Vec<MessageId> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM messages WHERE folder_id = ?1")?;
            stmt.query_map([&folder.0], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|id| !keep.contains(id.as_str()))
                .map(MessageId)
                .collect()
        };
        self.delete_messages(&stale)
    }

    pub fn put_body(&mut self, message: &MessageId, body: &Body) -> Result<()> {
        let content_type = match body.content_type {
            BodyType::Text => "text",
            BodyType::Html => "html",
        };
        self.conn.execute(
            "INSERT INTO bodies (message_id, content_type, body, fetched_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (message_id) DO UPDATE SET
                 content_type = excluded.content_type,
                 body = excluded.body,
                 fetched_at = excluded.fetched_at",
            (
                &message.0,
                content_type,
                &body.content,
                Timestamp::now().as_millisecond(),
            ),
        )?;
        Ok(())
    }

    /// The cached body of `message`, if it has been fetched.
    pub fn body(&self, message: &MessageId) -> Result<Option<Body>> {
        let body = self
            .conn
            .query_row(
                "SELECT content_type, body FROM bodies WHERE message_id = ?1",
                [&message.0],
                |row| {
                    Ok(Body {
                        content_type: match row.get::<_, String>(0)?.as_str() {
                            "html" => BodyType::Html,
                            _ => BodyType::Text,
                        },
                        content: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(body)
    }

    /// Messages in one folder, pinned then newest first.
    pub fn messages_in(&self, folder: &FolderId) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, folder_id, conversation_id, subject, from_name, from_addr,
                    received_at, is_read, flagged, importance, has_attachments, preview,
                    pinned
             FROM messages WHERE folder_id = ?1
             ORDER BY pinned DESC, received_at DESC",
        )?;
        let messages = stmt
            .query_map([&folder.0], message_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(messages)
    }

    /// The unread messages in one folder, newest first, along with those in
    /// `keep` even once they are read, so mail just read stays in the list.
    pub fn unread_in(&self, folder: &FolderId, keep: &[MessageId]) -> Result<Vec<Message>> {
        let mut messages = self.messages_in(folder)?;
        messages.retain(|m| !m.is_read || keep.contains(&m.id));
        Ok(messages)
    }

    /// Messages in any folder whose subject, sender, preview or fetched body
    /// contains `query`, ignoring case, pinned then newest first, at most
    /// `limit` of them.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Message>> {
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self.conn.prepare(
            "SELECT id, folder_id, conversation_id, subject, from_name, from_addr,
                    received_at, is_read, flagged, importance, has_attachments, preview,
                    pinned
             FROM messages m
             WHERE subject LIKE ?1 ESCAPE '\\'
                OR from_name LIKE ?1 ESCAPE '\\'
                OR from_addr LIKE ?1 ESCAPE '\\'
                OR preview LIKE ?1 ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM bodies b
                           WHERE b.message_id = m.id AND b.body LIKE ?1 ESCAPE '\\')
             ORDER BY pinned DESC, received_at DESC
             LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let messages = stmt
            .query_map((&pattern, limit), message_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(messages)
    }

    /// Where the next delta sync of `folder` resumes, if it has synced before.
    pub fn delta_link(&self, folder: &FolderId) -> Result<Option<String>> {
        let link = self
            .conn
            .query_row(
                "SELECT delta_link FROM folders WHERE id = ?1",
                [&folder.0],
                |row| row.get(0),
            )
            .optional()?;
        Ok(link.flatten())
    }

    pub fn set_delta_link(&mut self, folder: &FolderId, link: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE folders SET delta_link = ?2 WHERE id = ?1",
            (&folder.0, link),
        )?;
        Ok(())
    }

    /// Folders in folder tree order.
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, parent_id, name, well_known, unread, total FROM folders")?;
        let mut folders = stmt
            .query_map([], folder_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        sort_folders(&mut folders);
        Ok(folders)
    }
}

fn folder_from_row(row: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: FolderId(row.get(0)?),
        parent_id: row.get::<_, Option<String>>(1)?.map(FolderId),
        name: row.get(2)?,
        well_known: row
            .get::<_, Option<String>>(3)?
            .as_deref()
            .and_then(WellKnown::from_graph_name),
        unread: row.get(4)?,
        total: row.get(5)?,
    })
}

fn message_from_row(row: &Row) -> rusqlite::Result<Message> {
    let received_at = row.get(6)?;
    Ok(Message {
        id: MessageId(row.get(0)?),
        folder_id: FolderId(row.get(1)?),
        conversation_id: row.get(2)?,
        subject: row.get(3)?,
        from_name: row.get(4)?,
        from_addr: row.get(5)?,
        received_at: Timestamp::from_millisecond(received_at).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, Type::Integer, Box::new(e))
        })?,
        is_read: row.get(7)?,
        flagged: row.get(8)?,
        pinned: row.get(12)?,
        importance: importance_from_str(&row.get::<_, String>(9)?),
        has_attachments: row.get(10)?,
        preview: row.get(11)?,
    })
}

/// `text` with LIKE's wildcards made literal, for use with `ESCAPE '\'`.
fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn importance_to_str(importance: Importance) -> &'static str {
    match importance {
        Importance::Low => "low",
        Importance::Normal => "normal",
        Importance::High => "high",
    }
}

fn importance_from_str(s: &str) -> Importance {
    match s {
        "low" => Importance::Low,
        "high" => Importance::High,
        _ => Importance::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }
}
