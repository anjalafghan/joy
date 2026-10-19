-- A smarter jar: opening days and seasons, learned costs, real cab fares, vetoes.

-- Days a place is closed, as "mon,wed" (three-letter, lowercase). Empty = open daily.
ALTER TABLE activities ADD COLUMN closed_days TEXT NOT NULL DEFAULT '';
-- Only in season, as months 1–12; may wrap the new year (11 → 5 = Nov to May). NULL = all year.
ALTER TABLE activities ADD COLUMN season_from INTEGER;
ALTER TABLE activities ADD COLUMN season_to INTEGER;
-- The estimate was set from a real cost, so the learned factor shouldn't adjust it again.
ALTER TABLE activities ADD COLUMN est_from_actual INTEGER NOT NULL DEFAULT 0;

-- Real one-way cab fares per person (0 = PERSON_A, 1 = PERSON_B), each next to the
-- model's estimate at the time, so later rate or distance changes can't skew calibration.
ALTER TABLE history ADD COLUMN cab_fare_0 INTEGER;
ALTER TABLE history ADD COLUMN cab_est_0 INTEGER;
ALTER TABLE history ADD COLUMN cab_fare_1 INTEGER;
ALTER TABLE history ADD COLUMN cab_est_1 INTEGER;

-- "Not for me": either of us can take a slip out of the draw without deleting it.
CREATE TABLE vetoes (
    activity_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    person      INTEGER NOT NULL CHECK (person IN (0, 1)),
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (activity_id, person)
);

-- What we know about the seeded places (only where nothing is set yet).
UPDATE activities SET closed_days = 'mon' WHERE closed_days = '' AND title IN
    ('Planetarium show', 'Ferry to Elephanta Caves', 'Hike to Kanheri Caves');
UPDATE activities SET closed_days = 'wed' WHERE closed_days = '' AND title = 'Zoo visit';
UPDATE activities SET season_from = 11, season_to = 5 WHERE season_from IS NULL AND title = 'Flamingo boat ride';
UPDATE activities SET season_from = 10, season_to = 2 WHERE season_from IS NULL AND title = 'Karnala Bird Sanctuary hike';
UPDATE activities SET season_from = 6, season_to = 10 WHERE season_from IS NULL AND title = 'River rafting at Kolad';
