-- What we send each other on WhatsApp is a link to an envelope in Joy, not the date spelled
-- out in the chat. Each slip and each kept date gets a long random token for its envelope
-- (made the first time it's shared); the token is the only way in.
ALTER TABLE activities ADD COLUMN share_token TEXT;
ALTER TABLE history ADD COLUMN share_token TEXT;
CREATE UNIQUE INDEX activities_share_token ON activities (share_token) WHERE share_token IS NOT NULL;
CREATE UNIQUE INDEX history_share_token ON history (share_token) WHERE share_token IS NOT NULL;
