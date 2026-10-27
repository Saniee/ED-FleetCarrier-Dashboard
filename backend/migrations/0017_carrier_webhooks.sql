-- A carrier owner's Discord webhook. Its own table rather than columns on
-- `carriers`: carrier rows are served whole to anyone who may view the carrier,
-- and the URL is a secret (whoever holds it can post to the channel).
CREATE TABLE carrier_webhooks (
    carrier_id         bigint PRIMARY KEY
                         REFERENCES carriers(carrier_id) ON DELETE CASCADE,
    -- The account that set it up. Deliveries happen only while this account
    -- still owns the carrier, so a released or reassigned carrier stops posting
    -- to the previous owner's channel.
    owner_id           bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url                text NOT NULL,
    -- Which notices to send: jump, arrival, trade_order, order_filled, low_fuel.
    events             text[] NOT NULL,
    -- Tritium in tonnes; the low fuel notice fires when the level drops below
    -- it. 0 turns the notice off.
    low_fuel_threshold integer NOT NULL DEFAULT 200
                         CHECK (low_fuel_threshold BETWEEN 0 AND 1000),
    -- Switched off when Discord says the webhook is gone; saving again turns it
    -- back on.
    enabled            boolean NOT NULL DEFAULT true,
    last_sent_at       timestamptz,
    last_error         text,
    last_error_at      timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);
