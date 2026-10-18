-- Estimate vs actual. Each kept date remembers the estimate as it was when kept
-- (so updating the slip later doesn't rewrite history), and what it really cost.
ALTER TABLE history ADD COLUMN est_cost INTEGER;
ALTER TABLE history ADD COLUMN actual_cost INTEGER;
UPDATE history SET est_cost = (SELECT est_cost FROM activities WHERE id = history.activity_id)
WHERE outcome = 'did_it';
