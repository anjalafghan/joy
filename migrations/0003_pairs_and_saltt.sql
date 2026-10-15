-- Slips that go well together ("make it a pair"). Stored once per pair, smaller id first.
CREATE TABLE pairs (
    a_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    b_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    PRIMARY KEY (a_id, b_id),
    CHECK (a_id < b_id)
);

-- Saltt: the first research guessed SALT in Malad. The real one is out near Karjat,
-- which makes it a natural stop on a long drive. Only touches the untouched seed row.
UPDATE activities
SET venue = 'Saltt Restaurant & Bar',
    area = 'Karjat',
    lat = 18.9132,
    lng = 73.2964,
    setting = 'day_trip',
    duration = 'half_day',
    notes = 'Restaurant & bar out near Karjat. Best as the stop on a long drive. Check timings before you head out.'
WHERE title = 'Dinner at Saltt' AND venue = 'SALT (Inorbit Mall, Malad)';

INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(s.id, d.id), max(s.id, d.id)
FROM activities s, activities d
WHERE s.title = 'Dinner at Saltt' AND d.title = 'Long drive to Lonavala';
