ALTER TABLE carrier_events
    ADD carrier bigint REFERENCES carriers(carrier_id);