CREATE TABLE attachments (
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    position     INTEGER NOT NULL,
    name         TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size         INTEGER NOT NULL,
    kind         TEXT NOT NULL,
    PRIMARY KEY (message_id, id)
);

-- Bodies cached before attachments were kept would show none; fetch again.
DELETE FROM bodies
WHERE message_id IN (SELECT id FROM messages WHERE has_attachments = 1);
