-- True when the carrier's current location was last reported by someone other
-- than its owner (any user may report a CarrierJump / CarrierLocation for any
-- carrier). Cleared again by the next location event from the owner. Clients
-- show an asterisk next to the location while it is set.
ALTER TABLE carriers
    ADD location_unverified boolean NOT NULL DEFAULT false;
