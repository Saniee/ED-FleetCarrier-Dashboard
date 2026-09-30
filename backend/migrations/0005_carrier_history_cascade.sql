-- Deleting a carrier should take its event history with it, so a carrier can be
-- removed in one statement (from DBeaver, psql, or the app) without first
-- hand-deleting rows from carrier_events.
--
-- Note the other direction: carriers.last_event_id references carrier_events(id)
-- with NO ACTION. That does not block this, because the only row pointing at a
-- carrier's events is that same carrier's row, which the delete removes first.
ALTER TABLE carrier_events
    DROP CONSTRAINT carrier_events_carrier_fkey,
    ADD CONSTRAINT carrier_events_carrier_fkey
        FOREIGN KEY (carrier) REFERENCES carriers(carrier_id) ON DELETE CASCADE;
