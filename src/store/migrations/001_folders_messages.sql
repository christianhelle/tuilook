CREATE TABLE folders (
    id         TEXT PRIMARY KEY,
    parent_id  TEXT,
    name       TEXT NOT NULL,
    well_known TEXT,
    unread     INTEGER NOT NULL DEFAULT 0,
    total      INTEGER NOT NULL DEFAULT 0,
    delta_link TEXT
);

CREATE TABLE messages (
    id              TEXT PRIMARY KEY,
    folder_id       TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL,
    subject         TEXT NOT NULL,
    from_name       TEXT NOT NULL,
    from_addr       TEXT NOT NULL,
    received_at     INTEGER NOT NULL,
    is_read         INTEGER NOT NULL,
    flagged         INTEGER NOT NULL,
    importance      TEXT NOT NULL,
    has_attachments INTEGER NOT NULL,
    preview         TEXT NOT NULL
);

CREATE INDEX messages_by_folder ON messages(folder_id, received_at DESC);
