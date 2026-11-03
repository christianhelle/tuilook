-- What opening a message found out about it. A null event_id means it is not
-- an invite, so it need not be asked about again.
CREATE TABLE invites (
    message_id         TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    event_id           TEXT,
    start_at           INTEGER,
    end_at             INTEGER,
    all_day            INTEGER,
    location           TEXT,
    organizer          TEXT,
    answer             TEXT,
    response_requested INTEGER
);
