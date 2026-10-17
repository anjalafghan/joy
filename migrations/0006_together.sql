-- Who's holding the phone, plans with a time, surprises, photos, and a monthly budget.

-- Which of us (0 = PERSON_A, 1 = PERSON_B) drew and kept this date. NULL for older rows.
ALTER TABLE history ADD COLUMN drawn_by INTEGER;
-- When the date is actually happening (unix seconds), for the calendar invite.
ALTER TABLE history ADD COLUMN planned_at INTEGER;
-- A surprise: the other person only sees hints until it's revealed.
ALTER TABLE history ADD COLUMN surprise INTEGER NOT NULL DEFAULT 0;
ALTER TABLE history ADD COLUMN revealed INTEGER NOT NULL DEFAULT 0;
-- A photo lives on disk as <photos dir>/<history id>.jpg.
ALTER TABLE history ADD COLUMN has_photo INTEGER NOT NULL DEFAULT 0;

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
