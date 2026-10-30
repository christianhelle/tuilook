-- The calendar, cached for a window around today.
CREATE TABLE appointments (
    id           TEXT PRIMARY KEY,
    subject      TEXT NOT NULL,
    start_at     INTEGER NOT NULL,
    end_at       INTEGER NOT NULL,
    all_day      INTEGER NOT NULL,
    location     TEXT NOT NULL,
    organizer    TEXT NOT NULL,
    -- One address a line.
    attendees    TEXT NOT NULL,
    preview      TEXT NOT NULL,
    is_organizer INTEGER NOT NULL
);

CREATE INDEX appointments_by_start ON appointments (start_at);
