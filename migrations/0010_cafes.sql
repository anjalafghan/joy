-- Cafés people keep posting about (researched Oct 2026 from Mumbai food press and listings).
-- Costs are rough totals for two; hours change often, so notes say to check. Pins are approximate.
INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated) VALUES
  ('Matcha at Mokai', 'Two floors of Japanese-inspired cafe with a glass roof and a floating matcha bar upstairs. Try the Matcha Misu and the mochi. Moved to Pali Hill in 2026. (Check hours.)', 2000, 'local', 'Pali Hill', 'Mokai', 19.0662, 72.8318, 'short', 'foodie,cozy,romantic', 'low', 'joy', 1),
  ('Croissants at House of Croissant', 'A whole cafe built around laminated dough: filled croissants, croffles, good coffee. Open from 8:30 am. (Check hours.)', 900, 'local', 'Mount Mary', 'House of Croissant', 19.0560, 72.8288, 'short', 'foodie,chill', 'low', 'joy', 1),
  ('Coffee and bakes at Subko', 'The specialty roaster''s flagship in an old Ranwar cottage. Order something you''ve never tried from the brew bar. (Check hours.)', 900, 'local', 'Ranwar, Bandra', 'Subko', 19.0583, 72.8297, 'short', 'foodie,cozy,chill', 'low', 'joy', 1),
  ('Brunch at The Bagel Shop', 'A plant-filled, greenhouse-ish cafe near Carter Road. Bagel sandwiches and smoothies; dogs welcome. (Check hours.)', 1400, 'local', 'Pali Hill', 'The Bagel Shop', 19.0679, 72.8262, 'short', 'foodie,chill,cozy', 'low', 'joy', 1),
  ('Pardon Our French', 'Pooja Dhingra''s tiny Parisian-feeling cafe in Ballard Estate, about 22 seats. Brown butter cake and an Earl Grey matcha latte. (Check hours.)', 1500, 'local', 'Ballard Estate', 'Pardon Our French', 18.9362, 72.8402, 'short', 'foodie,romantic,cozy', 'low', 'joy', 1),
  ('Kala Ghoda Cafe', 'Skylit old-warehouse cafe with its own little gallery, on Ropewalk Lane. Slow coffee, then a wander through the art district.', 1000, 'local', 'Kala Ghoda', 'Kala Ghoda Cafe', 18.9289, 72.8320, 'short', 'foodie,cultural,chill', 'low', 'joy', 1),
  ('Cafe Mondegar and the Mario Miranda walls', 'The Colaba classic with Mario Miranda''s cartoons all over the walls. Find yourselves in the murals over a cold coffee.', 1300, 'local', 'Colaba', 'Cafe Mondegar', 18.9236, 72.8318, 'short', 'cultural,chill,music', 'med', 'joy', 1),
  ('Conçu in Colaba', 'Hyderabad''s much-loved patisserie, now in a tall, airy Colaba space: a cafe by day and a bar by evening. Share a slice of cake. (Price is a guess.)', 2500, 'local', 'Colaba', 'Conçu', 18.9138, 72.8262, 'short', 'foodie,romantic', 'low', 'joy', 1),
  ('Irish coffee at Prithvi Cafe', 'The open-air theatre cafe under the trees, since the 1970s. Irish coffee and the bun maska, before or after a play.', 600, 'local', 'Juhu', 'Prithvi Cafe', 19.1056, 72.8265, 'short', 'cultural,chill,cozy', 'low', 'joy', 1),
  ('Breakfast at Silver Beach Cafe', 'All-day cafe by Juhu beach with a breezy terrace. Go for a lazy breakfast, then walk on the sand. (Check hours.)', 2000, 'local', 'Juhu', 'Silver Beach Cafe', 19.1003, 72.8268, 'short', 'foodie,romantic,chill', 'low', 'joy', 1),
  ('Comics and coffee at Leaping Windows', 'A Versova cafe with a comic-book library downstairs. Each pick a comic for the other to read over coffee. (Check hours.)', 1400, 'local', 'Versova', 'Leaping Windows', 19.1330, 72.8168, 'short', 'cozy,games,chill', 'low', 'joy', 1),
  ('Pink tea at EL&N', 'The London cafe famous for walls of flowers and pink everything, at Jio World Plaza in BKC. Dress for the photos. (Check hours.)', 2500, 'local', 'BKC', 'EL&N London, Jio World Plaza', 19.0642, 72.8663, 'short', 'foodie,romantic', 'low', 'joy', 1);

-- Cafes that go well with something already in the jar.
INSERT OR IGNORE INTO pairs (a_id, b_id)
SELECT min(x.id, y.id), max(x.id, y.id)
FROM activities x, activities y
WHERE (x.title, y.title) IN (
  VALUES ('Irish coffee at Prithvi Cafe', 'A play at Prithvi Theatre'),
         ('Kala Ghoda Cafe', 'Jehangir Art Gallery and a Kala Ghoda wander'),
         ('Coffee and bakes at Subko', 'Street-art walk around Chapel Road'),
         ('Breakfast at Silver Beach Cafe', 'Juhu Beach street-food evening'),
         ('Comics and coffee at Leaping Windows', 'Ferry from Versova to Madh Island'),
         ('Matcha at Mokai', 'Sunset at Bandra Fort'),
         ('Pardon Our French', 'Fort heritage walk at night')
);
