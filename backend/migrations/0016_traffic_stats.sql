-- Traffic statistics for the admin dashboard.

-- When the server received each event. `timestamp` is the journal's time, which
-- says nothing about traffic (a journal replay sends days-old events). Rows
-- from before this migration stay NULL: their arrival time is unknown.
ALTER TABLE carrier_events ADD received_at timestamptz;
ALTER TABLE carrier_events ALTER received_at SET DEFAULT now();

-- Ingest requests per day, per reporting account (0: the legacy token or
-- anonymous ingest), per kind and outcome:
--   stored   written
--   ignored  accepted but not kept (an untracked carrier, a station market)
--   refused  turned away (another user's carrier, a callsign clash, a bad event)
-- No foreign key, so the numbers outlive a deleted account.
CREATE TABLE ingest_stats (
    day     date   NOT NULL,
    user_id bigint NOT NULL,
    kind    text   NOT NULL CHECK (kind IN ('carrier', 'market')),
    outcome text   NOT NULL CHECK (outcome IN ('stored', 'ignored', 'refused')),
    count   bigint NOT NULL,
    PRIMARY KEY (day, user_id, kind, outcome)
);
