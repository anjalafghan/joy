-- Plans don't always follow the usual days: an office Saturday is an evening after work, a
-- Monday off is a whole day from home. A kept date remembers which it was ("weeknight",
-- "weekend", "apart"); empty means whatever the schedule says for that date.
ALTER TABLE history ADD COLUMN day_kind TEXT NOT NULL DEFAULT '';
