CREATE TABLE activities (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT    NOT NULL,
    notes           TEXT    NOT NULL DEFAULT '',
    est_cost        INTEGER NOT NULL DEFAULT 0,      -- total for both of us, per outing
    setting         TEXT    NOT NULL,                -- home | local | day_trip | travel
    area            TEXT    NOT NULL DEFAULT '',
    venue           TEXT    NOT NULL DEFAULT '',
    lat             REAL,                             -- venue location; NULL = at home
    lng             REAL,
    duration        TEXT    NOT NULL,                -- short | half_day | full_day | weekend
    tags            TEXT    NOT NULL DEFAULT '',     -- comma separated, lowercase
    energy          TEXT    NOT NULL DEFAULT 'med',  -- low | med | high
    added_by        TEXT    NOT NULL DEFAULT '',
    archived        INTEGER NOT NULL DEFAULT 0,
    times_done      INTEGER NOT NULL DEFAULT 0,
    last_done_at    INTEGER,                          -- unix seconds
    last_skipped_at INTEGER,                          -- unix seconds
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    picked_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    outcome     TEXT    NOT NULL,                     -- did_it | skipped
    rating      INTEGER,                              -- 1..5
    note        TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX history_picked_at ON history(picked_at DESC);
