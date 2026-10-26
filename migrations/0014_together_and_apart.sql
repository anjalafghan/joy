-- "At home" splits in two: 'home' is at home together, in the same room; 'apart' is long
-- distance, each at our own place. The fifty long-distance slips move to 'apart', and their
-- temporary "long-distance" tag goes (the setting says it now).
UPDATE activities
SET setting = 'apart',
    tags = trim(replace(replace(replace(',' || tags || ',', ',long-distance,', ','), ',,', ','), ',,', ','), ',')
WHERE setting = 'home' AND (',' || tags || ',') LIKE '%,long-distance,%';

-- Twenty more for when we're in the same room, and ten more for when we're not.
-- Costs are rough totals for both of us. A title that's already in the jar is skipped.
WITH new (title, notes, est_cost, setting, duration, tags, energy) AS (VALUES
  ('Slow dance in the kitchen', 'Lights low, one song each, no talking about the dishes.', 0, 'home', 'short', 'romantic,music', 'low'),
  ('Massage night', 'Warm oil, a calm playlist, twenty minutes each. Take turns choosing who goes first.', 400, 'home', 'short', 'romantic,cozy,chill', 'low'),
  ('Board game marathon', 'A two-player game you''ve never played, like Codenames Duet or Patchwork, and best of five.', 1500, 'home', 'short', 'games,cozy', 'med'),
  ('Polaroid photoshoot at home', 'One roll, ten shots each, around the house. Stick the best ones on the fridge.', 1500, 'home', 'short', 'creative,romantic', 'med'),
  ('Breakfast in bed for each other', 'One of you cooks on Saturday, the other on Sunday. Tray, flowers, the works.', 500, 'home', 'short', 'romantic,foodie,cozy', 'low'),
  ('Paint one canvas together', 'One canvas, two brushes, take turns adding a stroke until it''s done. Hang it, however it turns out.', 700, 'home', 'short', 'creative,chill', 'low'),
  ('Bedsheet cinema', 'Hang a white sheet, borrow a projector or use the laptop, make popcorn and build a cinema on the floor.', 500, 'home', 'short', 'cozy,romantic', 'low'),
  ('Pizza from scratch', 'Make the dough, stretch it badly, each top half the pizza without telling the other.', 600, 'home', 'short', 'foodie,games', 'med'),
  ('Candlelit dinner, phones in a drawer', 'Cook something simple, set the table properly, and put both phones away till dessert.', 800, 'home', 'short', 'romantic,foodie', 'low'),
  ('Rearrange the room together', 'Move one big thing, then everything else around it. Agree to try it for a week.', 0, 'home', 'short', 'creative,active', 'high'),
  ('Learn a dance from a video', 'A short dance trend or a couple''s routine. Record the final take, keep it to yourselves.', 0, 'home', 'short', 'music,active,games', 'high'),
  ('Hot chocolate bar', 'Three kinds of chocolate, marshmallows, cinnamon, chilli. Build each other a mug.', 400, 'home', 'short', 'cozy,foodie', 'low'),
  ('Indoor picnic on the floor', 'Blanket, cushions, finger food and a playlist. No sitting on the sofa allowed.', 600, 'home', 'short', 'cozy,romantic,chill', 'low'),
  ('Teach each other a card game', 'Each teaches one game from childhood. Play until someone wins three in a row.', 0, 'home', 'short', 'games,chill', 'low'),
  ('Bake cookies for the neighbours', 'One batch for you, one to hand out. Ring the bell together.', 500, 'home', 'short', 'foodie,cozy', 'med'),
  ('Film a day in our life', 'Short clips all day, edit it together at night with your own soundtrack.', 0, 'home', 'half_day', 'creative,romantic', 'med'),
  ('Mocktail or cocktail lab', 'Buy three ingredients each, invent a drink each, name it after the other.', 800, 'home', 'short', 'foodie,games', 'med'),
  ('Start a herb box together', 'Basil, mint, coriander in one box on the windowsill. Cook with it in a month.', 500, 'home', 'short', 'creative,chill', 'low'),
  ('Tie-dye T-shirts', 'Two white tees and a dye kit, each makes one for the other. Wear them on the next date.', 800, 'home', 'short', 'creative,games', 'med'),
  ('Fold a hundred paper cranes', 'Learn one fold together, then race. String them up when you''re done.', 200, 'home', 'short', 'creative,chill', 'low'),

  ('Start a series, only on calls', 'Pick a show you''ll only ever watch together on a call. No cheating ahead.', 0, 'apart', 'short', 'cozy,chill', 'low'),
  ('Video-call scavenger hunt', 'Take turns calling out "something blue", "the oldest thing you own"… first to show it on camera scores.', 0, 'apart', 'short', 'games,active', 'high'),
  ('Online board game night', 'A shared online version of Ludo, Uno or Codenames. Loser picks the next movie.', 0, 'apart', 'short', 'games', 'med'),
  ('A voice-notes-only day', 'No texts all day, just voice notes, like little radio messages to each other.', 0, 'apart', 'full_day', 'romantic,chill', 'low'),
  ('Matching outfits on a call', 'Agree on a colour or a theme, get dressed apart, reveal at the same moment.', 0, 'apart', 'short', 'games,romantic', 'low'),
  ('Snack taste test, two cities', 'Five snacks each, the same list. Rate blind on a call and compare scores.', 500, 'apart', 'short', 'foodie,games', 'low'),
  ('Tour each other''s neighbourhood', 'Walk your street on a video call and show the other your chai stall, your shortcut, your view.', 0, 'apart', 'short', 'chill,active', 'med'),
  ('Journal side by side', 'Twenty quiet minutes on a call, writing about the week. Read one line each aloud.', 0, 'apart', 'short', 'chill,romantic', 'low'),
  ('Draw each other''s room from memory', 'Five minutes each, then show the drawing and the real room. Laugh at the furniture.', 0, 'apart', 'short', 'creative,games', 'low'),
  ('Learn the same song, apart', 'Pick one song and learn it on whatever you have, voice or guitar or keyboard. Perform it in a week.', 0, 'apart', 'short', 'music,creative', 'med')
)
INSERT INTO activities (title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, added_by, generated)
SELECT title, notes, est_cost, setting, '', '', NULL, NULL, duration, tags, energy, 'joy', 1
FROM new WHERE NOT EXISTS (SELECT 1 FROM activities a WHERE a.title = new.title);
