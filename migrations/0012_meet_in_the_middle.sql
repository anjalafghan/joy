-- "Meet in the middle": each of us says privately how tonight feels; the jar draws from
-- where our answers overlap. One row per person per local day; old days are deleted on
-- the next check-in, so nothing builds up.
CREATE TABLE checkins (
    who          INTEGER NOT NULL CHECK (who IN (0, 1)),
    day          TEXT    NOT NULL,                -- local YYYY-MM-DD
    energy       TEXT    NOT NULL,                -- low | med | high
    budget       INTEGER,                         -- NULL = any amount
    max_duration TEXT    NOT NULL DEFAULT '',     -- '' = any
    reach        TEXT    NOT NULL DEFAULT 'far',  -- home | near | town | far
    vibe         TEXT    NOT NULL DEFAULT '',     -- one tag, or '' = anything
    at           INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (who, day)
);
