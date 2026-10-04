-- The fleet carrier's market, split into the two parts the journal gives us:
--
--   * the `event`       - the header of the last `Market` event: which station,
--                         where, and when it was read. One row per carrier.
--   * the `commodities` - every item that event listed. Replaced wholesale each
--                         time, because a `Market` event is a full snapshot of
--                         the market rather than a delta.
--
-- They are two tables rather than one because they have different cardinality
-- and different lifetimes, and because the list is rewritten while the header is
-- upserted.
--
-- Identity: for a fleet carrier the journal's `MarketID` equals its `CarrierID`,
-- so `carrier_id` is the key throughout. Both tables cascade from `carriers`,
-- so removing a carrier takes its market with it.

CREATE TABLE carrier_markets (
    carrier_id             bigint PRIMARY KEY
                             REFERENCES carriers(carrier_id) ON DELETE CASCADE,
    -- The raw `MarketID` from the event. Equal to `carrier_id` for a fleet
    -- carrier; kept so the reported value is not lost.
    market_id              bigint NOT NULL,
    station_name           text,
    station_type           text,
    star_system            text,
    carrier_docking_access text,
    -- The event's own timestamp, stored as text like every other journal
    -- timestamp (see 0002).
    timestamp              text,
    updated_at             timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE market_commodities (
    carrier_id         bigint NOT NULL
                         REFERENCES carrier_markets(carrier_id) ON DELETE CASCADE,
    -- The journal's commodity id (`id`), stable across markets.
    commodity_id       bigint NOT NULL,
    -- Journal symbol, e.g. `$iridium_name;`. Always present.
    name               text NOT NULL,
    name_localised     text,
    category           text,
    category_localised text,
    -- Prices in credits. `buy_price` is what the carrier pays (its buy order),
    -- `sell_price` is what it charges. 0 means that side is not traded.
    buy_price          bigint,
    sell_price         bigint,
    mean_price         bigint,
    -- 0-3, 3 being the highest.
    stock_bracket      bigint,
    demand_bracket     bigint,
    stock              bigint,
    demand             bigint,
    consumer           boolean,
    producer           boolean,
    rare               boolean,
    PRIMARY KEY (carrier_id, commodity_id)
);
