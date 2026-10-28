-- Two lines on a kept date: one from each of us. Each line stays sealed from the other one
-- of us until both lines are written.
CREATE TABLE memories (
    history_id INTEGER NOT NULL,
    who        INTEGER NOT NULL CHECK (who IN (0, 1)),
    line       TEXT    NOT NULL,
    at         INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (history_id, who)
);

-- The savings jar: one pot of money we put aside for a slip too big for one month.
-- The balance is the sum. What it's for lives in settings ('saving_for' = activity id).
CREATE TABLE savings (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    amount INTEGER NOT NULL CHECK (amount != 0),
    -- add (by hand) | leftover (a month's leftover) | kept (paid for a date, negative) | back (a paid date was deleted)
    kind   TEXT    NOT NULL DEFAULT 'add',
    note   TEXT    NOT NULL DEFAULT '',
    on_day TEXT    NOT NULL DEFAULT (date('now', 'localtime'))
);

-- How much of a kept date the savings jar paid for. That part doesn't count against the
-- month's budget: it was set aside earlier.
ALTER TABLE history ADD COLUMN from_savings INTEGER;
