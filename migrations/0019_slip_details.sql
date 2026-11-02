-- What the day-of card needs from a slip: when it's open, when it's best, how long it really
-- takes, whether to book, whether rain matters, what to bring, and the station to aim for.

-- Opening hours as "tue-sun 12:00-23:30; mon 18:00-23:00". A close earlier than the open runs
-- past midnight. Empty = we don't know (the jar assumes open, the card says "check hours").
ALTER TABLE activities ADD COLUMN hours TEXT NOT NULL DEFAULT '';
-- morning | afternoon | evening | night | '' (any time).
ALTER TABLE activities ADD COLUMN best_time TEXT NOT NULL DEFAULT '';
-- How long the date itself takes, not counting travel. NULL = from `duration`.
ALTER TABLE activities ADD COLUMN minutes INTEGER;
-- none | weekends | always | '' (unknown).
ALTER TABLE activities ADD COLUMN booking TEXT NOT NULL DEFAULT '';
-- Book this many days ahead (when booking isn't none).
ALTER TABLE activities ADD COLUMN book_ahead_days INTEGER;
-- indoor | outdoor | mixed | '' (unknown). Plan B swaps an outdoor date for an indoor one.
ALTER TABLE activities ADD COLUMN cover TEXT NOT NULL DEFAULT '';
-- "closed shoes, cash": becomes the little jobs we deal each other.
ALTER TABLE activities ADD COLUMN bring TEXT NOT NULL DEFAULT '';
-- The nearest local or metro station ("Andheri"), and the walk from it in minutes.
ALTER TABLE activities ADD COLUMN station TEXT NOT NULL DEFAULT '';
ALTER TABLE activities ADD COLUMN station_walk_min INTEGER;

-- What we know without looking anything up: dates at home or apart happen indoors, with
-- nothing to book.
UPDATE activities SET cover = 'indoor', booking = 'none' WHERE setting IN ('home', 'apart');
