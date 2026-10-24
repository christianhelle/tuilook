//! Local SQLite cache. The UI reads from it; only core writes to it.

use std::sync::LazyLock;

use rusqlite::{Connection, Row};
use rusqlite_migration::{M, Migrations};
use tuilook_model::{Folder, FolderId, WellKnown, sort_folders};

static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
    Migrations::new(vec![M::up(include_str!(
        "../migrations/001_folders_messages.sql"
    ))])
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
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(Self { conn })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }
}
