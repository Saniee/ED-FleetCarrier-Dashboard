-- The journal reports carrier tax as five per-service rates, never a single
-- value, so replace the assumed `tax_rate` column with the real set.
ALTER TABLE carriers
    DROP COLUMN IF EXISTS tax_rate,
    ADD COLUMN tax_rate_shipyard   bigint,
    ADD COLUMN tax_rate_rearm      bigint,
    ADD COLUMN tax_rate_outfitting bigint,
    ADD COLUMN tax_rate_refuel     bigint,
    ADD COLUMN tax_rate_repair     bigint;
