-- Twenty more suggested plans ("generated for you"). Costs are rough totals for two;
-- notes say what to check. Pins are approximate.
INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated) VALUES
  -- at home
  ('Recreate your first date at home', 'Same food, same music, same outfit if you can find it. Swap what you each remember.', 500, 'home', '', '', NULL, NULL, 'short', 'romantic,cozy', 'low', 'joy', 1),
  ('Bollywood karaoke night in the living room', 'Pick a decade, find karaoke versions online, and score each other out of 10.', 0, 'home', '', '', NULL, NULL, 'short', 'music,games', 'high', 'joy', 1),
  ('Blind taste test', 'Five kinds of chips, chocolate or chai, labels hidden. Loser plans the next date.', 500, 'home', '', '', NULL, NULL, 'short', 'foodie,games', 'med', 'joy', 1),
  ('Make each other a playlist', 'Ten songs each that say something. Listen together and explain one pick each.', 0, 'home', '', '', NULL, NULL, 'short', 'music,romantic', 'low', 'joy', 1),
  ('Start a two-person book club', 'Pick a short book, read a chapter a day, and talk it over with chai on Sunday.', 600, 'home', '', '', NULL, NULL, 'short', 'cultural,cozy', 'low', 'joy', 1),
  -- around town
  ('Street-art walk around Chapel Road', 'Bandra''s mural lanes. Go in the morning light and end with breakfast nearby. (Pin approximate.)', 400, 'local', 'Bandra West', 'Chapel Road murals', 19.0577, 72.8302, 'short', 'creative,outdoors', 'med', 'joy', 1),
  ('Banganga Tank and the Walkeshwar lanes', 'A quiet old tank with steps and temples, hidden in Malabar Hill. Go early. (Pin approximate.)', 200, 'local', 'Malabar Hill', 'Banganga Tank', 18.9446, 72.7937, 'short', 'cultural,chill,outdoors', 'low', 'joy', 1),
  ('Morning at the Hanging Gardens', 'Walk the gardens and the old-shoe park next door, then a view over Marine Drive. (Pin approximate.)', 0, 'local', 'Malabar Hill', 'Hanging Gardens', 18.9568, 72.8050, 'short', 'outdoors,chill', 'low', 'joy', 1),
  ('Breakfast at Kyani & Co.', 'Old Irani café: bun maska, akuri, chai. Closed some days, so check before you go. (Pin approximate.)', 500, 'local', 'Dhobi Talao', 'Kyani & Co.', 18.9437, 72.8279, 'short', 'foodie,cozy,cultural', 'low', 'joy', 1),
  ('Jehangir Art Gallery and a Kala Ghoda wander', 'Free gallery shows, then the lanes, cafés and bookshops around Kala Ghoda. (Check what''s showing.)', 600, 'local', 'Kala Ghoda', 'Jehangir Art Gallery', 18.9276, 72.8317, 'half_day', 'cultural,creative,chill', 'low', 'joy', 1),
  ('Sunrise at Sassoon Docks', 'The fish market at full speed at dawn. Wear shoes you don''t mind, then breakfast in Colaba. (Check photography rules.)', 500, 'local', 'Colaba', 'Sassoon Docks', 18.9106, 72.8273, 'short', 'adventurous,foodie', 'med', 'joy', 1),
  ('Juhu Beach street-food evening', 'Pav bhaji, gola and a walk on the sand at dusk. Go on a weekday to dodge crowds.', 500, 'local', 'Juhu', 'Juhu Beach', 19.0990, 72.8265, 'short', 'foodie,outdoors,romantic', 'med', 'joy', 1),
  ('A show at the Royal Opera House', 'Restored 1911 opera house. Concerts, plays and occasional tours. (Check the calendar and dress up a little.)', 1500, 'local', 'Girgaon', 'Royal Opera House', 18.9562, 72.8189, 'short', 'cultural,romantic,music', 'low', 'joy', 1),
  ('Escape room challenge', 'Pick a one-hour room rated medium. Agree beforehand who reads clues and who searches.', 2000, 'local', '', '', NULL, NULL, 'short', 'games,adventurous', 'high', 'joy', 1),
  ('Bowling night', 'Loser of each game buys the next round of fries.', 1200, 'local', '', '', NULL, NULL, 'short', 'games,active', 'high', 'joy', 1),
  ('Ice-cream crawl', 'Three scoops from three different places, one flavour neither of you would normally order.', 600, 'local', '', '', NULL, NULL, 'short', 'foodie,chill', 'low', 'joy', 1),
  -- day trips
  ('Karnala Bird Sanctuary hike', 'Forest trail up to the Karnala fort pinnacle, best Oct to Feb. Start early, carry water. (Check timings.)', 800, 'day_trip', 'Panvel', 'Karnala Bird Sanctuary', 18.8867, 73.1153, 'half_day', 'outdoors,active,adventurous', 'high', 'joy', 1),
  ('Vasai Fort and the beach', 'Portuguese-era fort ruins overgrown with trees, then the beach nearby. (Pin approximate.)', 800, 'day_trip', 'Vasai', 'Vasai Fort', 19.3290, 72.8148, 'half_day', 'cultural,outdoors', 'med', 'joy', 1),
  ('River rafting at Kolad', 'Kundalika river, usually monsoon to early winter when the dam releases water. Book an operator. (Check the season.)', 4000, 'day_trip', 'Kolad', 'Kundalika river', 18.4300, 73.3200, 'full_day', 'adventurous,outdoors,active', 'high', 'joy', 1),
  -- travel
  ('A slow weekend in Alibaug', 'Ferry from the Gateway to Mandwa, then a homestay near the beach. No plans beyond the sunset. (Check ferry timings.)', 8000, 'travel', 'Alibaug', 'Mandwa jetty', 18.8051, 72.8796, 'weekend', 'romantic,chill,outdoors', 'low', 'joy', 1);

-- A few natural pairs.
INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(x.id, y.id), max(x.id, y.id)
FROM activities x, activities y
WHERE (x.title, y.title) IN (
  VALUES ('Sunrise at Sassoon Docks', 'Breakfast at Kyani & Co.'),
         ('Banganga Tank and the Walkeshwar lanes', 'Morning at the Hanging Gardens'),
         ('Jehangir Art Gallery and a Kala Ghoda wander', 'Breakfast at Kyani & Co.'),
         ('Juhu Beach street-food evening', 'A play at Prithvi Theatre'),
         ('Street-art walk around Chapel Road', 'Ice-cream crawl')
);
