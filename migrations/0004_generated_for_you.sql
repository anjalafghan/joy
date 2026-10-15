-- Ideas the app suggested (not ones we wrote ourselves). Shown as "generated for you",
-- and putting one back doesn't use up a peek.
ALTER TABLE activities ADD COLUMN generated INTEGER NOT NULL DEFAULT 0;

INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated) VALUES
  ('A play at Prithvi Theatre', 'Book online; shows sell out on weekends. Irish coffee at Prithvi Café before the show. (Check what''s on.)', 1200, 'local', 'Juhu', 'Prithvi Theatre', 19.1067, 72.8265, 'short', 'cultural,romantic,cozy', 'low', 'joy', 1),
  ('Ferry to Elephanta Caves', 'Ferry from the Gateway of India (~1 hr each way). Closed Mondays. Go early, carry water. (Check ferry timings.)', 1500, 'day_trip', 'Elephanta Island', 'Elephanta Caves', 18.9633, 72.9315, 'half_day', 'cultural,outdoors,adventurous', 'med', 'joy', 1),
  ('Flamingo boat ride', 'Thane Creek, roughly Nov to May, best near high tide. Book the boat in advance. (Check the season before going.)', 1200, 'local', 'Airoli', 'Coastal & Marine Biodiversity Centre', 19.1415, 72.9786, 'half_day', 'outdoors,romantic,chill', 'low', 'joy', 1),
  ('Hike to Kanheri Caves', 'Inside Sanjay Gandhi National Park. Start early; cabs or cycles from the gate. Closed Mondays. (Check timings.)', 400, 'local', 'Borivali East', 'Kanheri Caves', 19.2058, 72.9069, 'half_day', 'outdoors,cultural,active', 'high', 'joy', 1),
  ('Late-night food walk on Mohammed Ali Road', 'Malpua, kebabs and phirni. Best after dark, and legendary during Ramzan. Go hungry. (Pin approximate.)', 1000, 'local', 'Bhendi Bazaar', 'Mohammed Ali Road', 18.9580, 72.8330, 'short', 'foodie,nightlife,adventurous', 'med', 'joy', 1),
  ('Matheran day trip', 'No cars up top, just forest trails and viewpoints. Toy train from Neral when it''s running. Same side as Saltt. (Check the toy train.)', 3000, 'day_trip', 'Matheran', 'Matheran hill station', 18.9866, 73.2685, 'full_day', 'outdoors,romantic,adventurous', 'med', 'joy', 1),
  ('Cook a dish from each other''s childhood', 'Each of you teaches the other one recipe you grew up with. Call home for the secret ingredient.', 600, 'home', '', '', NULL, NULL, 'short', 'foodie,cozy,romantic', 'low', 'joy', 1),
  ('Write letters to open in a year', 'Seal them, date them, hide them somewhere you''ll both remember. Add a photo from today.', 100, 'home', '', '', NULL, NULL, 'short', 'romantic,cozy,creative', 'low', 'joy', 1);

-- Saltt is out near Karjat: it pairs with a drive up to Matheran too.
INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(s.id, m.id), max(s.id, m.id)
FROM activities s, activities m
WHERE s.title = 'Dinner at Saltt' AND m.title = 'Matheran day trip';
