-- Carrier moderation: an admin can hide a carrier whatever its owner's
-- visibility, and audit entries can be about a carrier as well as an account.

-- Hidden carriers answer 404 to everyone but their owner (who sees the reason)
-- and admins, and are left out of the public list.
ALTER TABLE carriers
    ADD moderation_hidden_at timestamptz,
    ADD moderation_reason    text;

-- What `target_id` points at: 'user' or 'carrier' (a carrier_id). NULL with no
-- target (settings changes).
ALTER TABLE admin_actions
    ADD target_kind text CHECK (target_kind IN ('user', 'carrier'));

UPDATE admin_actions SET target_kind = 'user' WHERE target_id IS NOT NULL;

ALTER TABLE admin_actions
    ADD CONSTRAINT admin_actions_target_pair_check
        CHECK ((target_id IS NULL) = (target_kind IS NULL));

CREATE INDEX admin_actions_target_idx ON admin_actions (target_kind, target_id);

-- A carrier's history by newest first (the recent-events read), and what the
-- cascade walks when an admin deletes a carrier.
CREATE INDEX carrier_events_carrier_idx ON carrier_events (carrier, id);
