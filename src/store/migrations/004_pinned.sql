-- Pinned mail stays at the top of the message list.
ALTER TABLE messages ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
