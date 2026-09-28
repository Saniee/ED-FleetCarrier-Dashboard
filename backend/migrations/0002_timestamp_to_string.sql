ALTER TABLE carrier_events
    ALTER COLUMN timestamp TYPE text
    USING to_char(timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"');

ALTER TABLE carriers
    ALTER COLUMN purchased_at TYPE text
    USING to_char(purchased_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"');

ALTER TABLE carriers
    ALTER COLUMN jump_departure_time TYPE text
    USING to_char(jump_departure_time AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"');