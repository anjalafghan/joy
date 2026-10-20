-- Twenty more suggested plans ("generated for you"). Costs are rough totals for two;
-- notes say what to check. Pins are approximate.
INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated, closed_days, season_from, season_to) VALUES
  -- at home
  ('Blanket fort and a movie inside it', 'Sheets, cushions, fairy lights, snacks. The rule: phones stay outside the fort.', 300, 'home', '', '', NULL, NULL, 'short', 'cozy,romantic', 'low', 'joy', 1, '', NULL, NULL),
  ('The 36 questions', 'The psychologists'' list of questions that bring two people closer. Take turns, no skipping, end with four minutes of eye contact.', 0, 'home', '', '', NULL, NULL, 'short', 'romantic,chill', 'low', 'joy', 1, '', NULL, NULL),
  ('Spa night at home', 'Face masks, a foot soak, a slow massage each, and something calm playing. Book the bathroom for an hour.', 600, 'home', '', '', NULL, NULL, 'short', 'cozy,chill,romantic', 'low', 'joy', 1, '', NULL, NULL),
  ('Two-person bake-off', 'Same recipe, separate bowls, one oven. Taste blind and argue about the winner.', 500, 'home', '', '', NULL, NULL, 'short', 'foodie,games', 'med', 'joy', 1, '', NULL, NULL),
  ('Learn a salsa step from a video', 'Push the sofa back, pick a beginner tutorial, and don''t stop until you can do one turn without laughing.', 0, 'home', '', '', NULL, NULL, 'short', 'music,active,romantic', 'high', 'joy', 1, '', NULL, NULL),
  ('Make a scrapbook of us', 'Print twenty photos from your phones and fill a notebook with them, ticket stubs and a line under each.', 800, 'home', '', '', NULL, NULL, 'short', 'creative,cozy,romantic', 'low', 'joy', 1, '', NULL, NULL),
  -- around town
  ('Dr. Bhau Daji Lad Museum', 'Mumbai''s oldest museum, all gold and tiles, inside the Byculla zoo gardens. Usually closed Wednesdays. (Check timings.)', 300, 'local', 'Byculla', 'Dr. Bhau Daji Lad Museum', 18.9787, 72.8355, 'short', 'cultural,chill', 'low', 'joy', 1, 'wed', NULL, NULL),
  ('Walk out to Haji Ali at sunset', 'The causeway to the dargah over the sea, then a fresh juice at the corner. Only walkable at low tide, so check the tide.', 400, 'local', 'Haji Ali', 'Haji Ali Dargah', 18.9827, 72.8089, 'short', 'cultural,outdoors,romantic', 'low', 'joy', 1, '', NULL, NULL),
  ('Evening by Powai Lake', 'Walk the promenade as the lights come on, then dinner in Hiranandani. (Pin approximate.)', 1500, 'local', 'Powai', 'Powai Lake', 19.1270, 72.9050, 'short', 'chill,outdoors,romantic', 'low', 'joy', 1, '', NULL, NULL),
  ('Sunset at Bandra Fort', 'Old Portuguese ramparts over the Sea Link. Find a spot on the rocks and stay till the lights come on.', 200, 'local', 'Bandra West', 'Castella de Aguada', 19.0420, 72.8187, 'short', 'outdoors,romantic,chill', 'low', 'joy', 1, '', NULL, NULL),
  ('Berry pulao at Britannia & Co.', 'Parsi café in Ballard Estate, lunch only and closed Sundays. Order the berry pulao and caramel custard. (Check hours.)', 1800, 'local', 'Ballard Estate', 'Britannia & Co.', 18.9346, 72.8400, 'short', 'foodie,cultural,cozy', 'low', 'joy', 1, 'sun', NULL, NULL),
  ('Ferry from Versova to Madh Island', 'A five-minute boat across the creek, then the quiet beaches and lanes on the other side. (Check ferry timings.)', 600, 'local', 'Versova', 'Versova jetty', 19.1336, 72.8137, 'half_day', 'adventurous,outdoors,chill', 'med', 'joy', 1, '', NULL, NULL),
  ('Fort heritage walk at night', 'Flora Fountain, the High Court, the University and the Asiatic Society steps, all lit up. Bring chai.', 300, 'local', 'Fort', 'Asiatic Society steps', 18.9317, 72.8362, 'short', 'cultural,outdoors,romantic', 'med', 'joy', 1, '', NULL, NULL),
  ('Chor Bazaar treasure hunt', '₹500 each to find the strangest thing you can for the other. Many shops close on Fridays. (Bargain kindly.)', 1000, 'local', 'Bhendi Bazaar', 'Chor Bazaar', 18.9602, 72.8275, 'short', 'adventurous,games,cultural', 'med', 'joy', 1, 'fri', NULL, NULL),
  ('Go to a gig neither of you knows', 'Pick a small live show this week from a listings app, by a band you''ve never heard of.', 2000, 'local', '', '', NULL, NULL, 'short', 'music,nightlife', 'high', 'joy', 1, '', NULL, NULL),
  ('Eat a cuisine neither of you has tried', 'Burmese, Ethiopian, Naga, Lebanese: whatever''s new to both of you. Order the thing you can''t pronounce.', 2000, 'local', '', '', NULL, NULL, 'short', 'foodie,adventurous', 'med', 'joy', 1, '', NULL, NULL),
  -- day trips
  ('Global Vipassana Pagoda and Gorai beach', 'A huge golden dome you can sit inside in silence, then the beach. Take the ferry from Borivali. (Check timings.)', 800, 'day_trip', 'Gorai', 'Global Vipassana Pagoda', 19.2286, 72.8059, 'half_day', 'cultural,chill,outdoors', 'low', 'joy', 1, '', NULL, NULL),
  ('Kondana Caves trek', 'Old Buddhist caves near Karjat with a waterfall in front during and after the monsoon. Moderate trail. (Pin approximate.)', 1500, 'day_trip', 'Karjat', 'Kondana Caves', 18.8318, 73.3873, 'full_day', 'outdoors,active,adventurous', 'high', 'joy', 1, '', 6, 2),
  -- travel
  ('Vineyard weekend in Nashik', 'Wine tours and tastings at the vineyards, a slow lunch, a sunset over the vines. (Book a tasting.)', 12000, 'travel', 'Nashik', 'Sula Vineyards', 20.0068, 73.6866, 'weekend', 'romantic,foodie,chill', 'low', 'joy', 1, '', NULL, NULL),
  ('Fireflies at Bhandardara', 'For a few weeks before the monsoon the trees light up with fireflies. Camp by the lake. (Check the season each year.)', 7000, 'travel', 'Bhandardara', 'Arthur Lake', 19.5432, 73.7550, 'weekend', 'adventurous,outdoors,romantic', 'med', 'joy', 1, '', 5, 6);

-- A few natural pairs.
INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(x.id, y.id), max(x.id, y.id)
FROM activities x, activities y
WHERE (x.title, y.title) IN (
  VALUES ('Sunset at Bandra Fort', 'Street-art walk around Chapel Road'),
         ('Berry pulao at Britannia & Co.', 'Fort heritage walk at night'),
         ('Chor Bazaar treasure hunt', 'Dr. Bhau Daji Lad Museum'),
         ('Blanket fort and a movie inside it', 'Spa night at home'),
         ('Evening by Powai Lake', 'Eat a cuisine neither of you has tried')
);
