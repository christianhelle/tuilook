CREATE TABLE bodies (
    message_id   TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    content_type TEXT NOT NULL,
    body         TEXT NOT NULL,
    fetched_at   INTEGER NOT NULL
);
