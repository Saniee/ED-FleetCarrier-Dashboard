-- Callsigns are looked up case-insensitively (`upper(callsign) = upper($1)`),
-- but the UNIQUE constraint from 0001 compared them as typed, so 'KLJ-97Z' and
-- 'klj-97z' could both exist and a lookup would return whichever came first.
-- Enforce uniqueness on the same expression the lookup uses.
ALTER TABLE carriers DROP CONSTRAINT carriers_callsign_key;

CREATE UNIQUE INDEX carriers_callsign_upper_idx ON carriers (upper(callsign));
