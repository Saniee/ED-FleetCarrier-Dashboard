-- Owner finances: what upkeep costs, and when to warn about it.

-- Weekly upkeep in credits, as the in-game finance screen shows it. The journal
-- never reports upkeep and the per-service prices aren't published reliably,
-- so the owner enters it. NULL means not set: no runway, no low balance notice.
-- On the carrier row so the live feed can work out runway as the balance moves;
-- the balance itself is already there.
ALTER TABLE carriers
    ADD COLUMN weekly_upkeep bigint
        CHECK (weekly_upkeep > 0 AND weekly_upkeep <= 1000000000);

-- The low balance notice fires when the balance drops below this many weeks of
-- upkeep. 0 turns it off.
ALTER TABLE carrier_webhooks
    ADD COLUMN low_balance_weeks integer NOT NULL DEFAULT 4
        CHECK (low_balance_weeks BETWEEN 0 AND 52);

-- Balance history reads only the three events that report a balance, newest
-- window first; the existing indexes are by name across all carriers or by id.
CREATE INDEX carrier_events_balance_idx ON carrier_events (carrier, timestamp)
    WHERE event_name IN ('CarrierStats', 'CarrierFinance', 'CarrierBankTransfer');
