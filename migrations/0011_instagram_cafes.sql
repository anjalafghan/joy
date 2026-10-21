-- Cafés that keep coming up in Mumbai café posts on Instagram (Oct 2026), each checked
-- against a second source. Costs are rough totals for two; pins are approximate.
INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated, closed_days) VALUES
  ('Butter dosas at Benne', 'Bangalore-style benne dosas, soft and crisp and full of butter, with filter coffee. Closed Mondays; there''s often a wait. (Check hours.)', 600, 'local', 'Bandra West', 'Benne Dosa, 16th Road', 19.0640, 72.8345, 'short', 'foodie,cozy', 'low', 'joy', 1, 'mon'),
  ('Pizza and coffee at Dough & Joe', 'A big neighbourhood café on Hill Road: slow-fermented pizza to share, then a specialty coffee each. (Check hours.)', 1800, 'local', 'Bandra West', 'Dough & Joe, Hill Road', 19.0552, 72.8335, 'short', 'foodie,chill', 'low', 'joy', 1, ''),
  ('Brunch at Boojee Cafe', 'The Bandra brunch spot people queue for. Go early on a weekend, order two plates and swap halfway. (Check hours.)', 1200, 'local', 'Bandra West', 'Boojee Cafe, New Kantwadi Road', 19.0522, 72.8253, 'short', 'foodie,cozy,chill', 'low', 'joy', 1, ''),
  ('Coffee on tap at Bokka', 'A tiny specialty café pouring India''s first coffee on tap. Limited seats, so take it to the lanes if it''s full. (Check hours.)', 800, 'local', 'Bandra West', 'Bokka Coffee, 16th Road', 19.0646, 72.8338, 'short', 'foodie,chill', 'low', 'joy', 1, ''),
  ('Grey Soul Coffee in Kala Ghoda', 'The roaster''s newest flagship. Try a light roast neither of you would pick, then wander the art district. (Pin approximate.)', 900, 'local', 'Kala Ghoda', 'Grey Soul Coffee', 18.9283, 72.8327, 'short', 'foodie,cultural,chill', 'low', 'joy', 1, ''),
  ('Cookies and cupcakes at NotFar', 'A Bandra bake shop and café from the CrumbleDoh founders. Pick a box to share and a coffee each. (Pin approximate.)', 800, 'local', 'Bandra West', 'NotFar', 19.0600, 72.8300, 'short', 'foodie,cozy', 'low', 'joy', 1, ''),
  ('Hot chocolate at Magari', 'A cosy, European-feeling café with books around. Thick hot chocolate and a long chat. (Price is a guess; check hours.)', 1200, 'local', 'Andheri West', 'Magari, Four Bungalows', 19.1320, 72.8255, 'short', 'cozy,chill,romantic', 'low', 'joy', 1, ''),
  ('Croffles at The Croffle Guys', 'The viral croissant-waffles from Shark Tank. Get one sweet and one savoury and split both. (Check hours.)', 600, 'local', 'Andheri West', 'The Croffle Guys, Lokhandwala', 19.1418, 72.8292, 'short', 'foodie,games', 'low', 'joy', 1, '');

INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(x.id, y.id), max(x.id, y.id)
FROM activities x, activities y
WHERE (x.title, y.title) IN (
  VALUES ('Grey Soul Coffee in Kala Ghoda', 'Jehangir Art Gallery and a Kala Ghoda wander'),
         ('Coffee on tap at Bokka', 'Street-art walk around Chapel Road'),
         ('Brunch at Boojee Cafe', 'Sunset at Bandra Fort')
);
