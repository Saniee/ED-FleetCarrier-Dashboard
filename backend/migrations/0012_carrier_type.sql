-- The journal's CarrierType: 'FleetCarrier' or 'SquadronCarrier'. Squadron
-- carriers are lighter, which changes the tritium maths. Plain text, as sent,
-- so a type the game adds later is stored rather than rejected.
--
-- is_squadron is separate: the owner's own designation that this carrier serves
-- as their squadron's (a new squadron often runs on its founder's personal
-- carrier), whatever hull the game says it is.
ALTER TABLE carriers
    ADD carrier_type text NOT NULL DEFAULT 'FleetCarrier',
    ADD is_squadron  boolean NOT NULL DEFAULT false;
