-- The day-of plan for a kept date: a Plan B the jar picked, and the little jobs it dealt us.

-- A slip to swap to if the weather turns or the place is shut. Chosen when the date is kept.
ALTER TABLE history ADD COLUMN plan_b INTEGER REFERENCES activities(id) ON DELETE SET NULL;

-- One small job each for a kept date: "Book a table", "Bring the speaker". Dealt to whoever has
-- had fewer, so it evens out over time.
CREATE TABLE tasks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    history_id INTEGER NOT NULL REFERENCES history(id) ON DELETE CASCADE,
    who        INTEGER NOT NULL CHECK (who IN (0, 1)),
    task       TEXT    NOT NULL,
    -- Local "YYYY-MM-DD" to do it by; empty = before the date.
    due        TEXT    NOT NULL DEFAULT '',
    done_at    INTEGER
);

CREATE INDEX tasks_history ON tasks(history_id);
