-- Deleting an event row should not be blocked by the carrier that recorded it as
-- `last_event_id`. With NO ACTION, the newest event for a carrier could never be
-- deleted (the carrier always points at it). SET NULL just forgets the cursor.
--
-- This complements 0005, which cascades the other direction (deleting a carrier
-- removes its history). Note the two never conflict: when a carrier is deleted,
-- its own row is gone before its events cascade away, so there is nothing left
-- for SET NULL to update.
ALTER TABLE carriers
    DROP CONSTRAINT carriers_last_event_id_fkey,
    ADD CONSTRAINT carriers_last_event_id_fkey
        FOREIGN KEY (last_event_id) REFERENCES carrier_events(id) ON DELETE SET NULL;
