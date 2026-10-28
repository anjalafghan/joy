-- How each month started: the budget then (a snapshot, so changing it later doesn't rewrite
-- the past) and leftover money rolled over from the month before (0 = started fresh).
CREATE TABLE month_budget (
    month TEXT PRIMARY KEY,           -- 'YYYY-MM'
    base  INTEGER NOT NULL,
    carry INTEGER NOT NULL DEFAULT 0
);
