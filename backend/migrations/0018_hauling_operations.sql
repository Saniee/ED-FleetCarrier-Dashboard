-- Hauling operations: a named goal on a carrier ("build the Col 285 outpost"),
-- with commodity targets and the deliveries that count towards them.

-- The latest state of each colonisation construction depot any plugin has
-- reported (`ColonisationConstructionDepot`). Shared: everyone docked at a depot
-- sees the same numbers, so the newest report wins.
CREATE TABLE construction_depots (
    market_id    bigint PRIMARY KEY,
    -- Not in the event; the plugin adds where the commander was docked.
    star_system  text,
    station_name text,
    -- 0..1
    progress     double precision NOT NULL DEFAULT 0,
    complete     boolean NOT NULL DEFAULT false,
    failed       boolean NOT NULL DEFAULT false,
    -- [{ commodity, name, required, provided, payment }]; `commodity` is the
    -- lower-case key (`steel`), as in `operation_targets`.
    resources    jsonb NOT NULL DEFAULT '[]',
    -- The event's own time, to keep an older report from overwriting a newer one.
    reported_at  timestamptz NOT NULL,
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX construction_depots_system_idx ON construction_depots (lower(star_system));

CREATE TABLE operations (
    id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    carrier_id      bigint NOT NULL REFERENCES carriers(carrier_id) ON DELETE CASCADE,
    created_by      bigint REFERENCES users(id) ON DELETE SET NULL,
    name            text NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    -- Set for a colonisation operation: its targets are the depot's
    -- requirements. Without one the owner lists the targets.
    depot_market_id bigint REFERENCES construction_depots(market_id) ON DELETE SET NULL,
    status          text NOT NULL DEFAULT 'active'
                      CHECK (status IN ('active', 'complete', 'archived')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    completed_at    timestamptz
);

CREATE INDEX operations_carrier_idx ON operations (carrier_id, status);
CREATE INDEX operations_depot_idx ON operations (depot_market_id) WHERE status = 'active';

-- Targets of an operation without a depot.
CREATE TABLE operation_targets (
    operation_id bigint NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    commodity    text NOT NULL,
    name         text NOT NULL,
    required     integer NOT NULL CHECK (required > 0),
    PRIMARY KEY (operation_id, commodity)
);

-- What each account's plugin saw its commander deliver: sold to a carrier's
-- market (`MarketSell`, kind 'carrier') or handed to a construction depot
-- (`ColonisationContribution`, kind 'depot'). Operations count those since
-- they were created, so a delivery isn't tied to one operation.
CREATE TABLE hauling_deliveries (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id      bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind         text NOT NULL CHECK (kind IN ('carrier', 'depot')),
    market_id    bigint NOT NULL,
    commodity    text NOT NULL,
    name         text,
    amount       integer NOT NULL CHECK (amount > 0),
    delivered_at timestamptz NOT NULL,
    -- The plugin resends after a failure and journals get replayed: the same
    -- delivery must count once.
    UNIQUE (user_id, kind, market_id, commodity, delivered_at)
);

CREATE INDEX hauling_deliveries_market_idx ON hauling_deliveries (kind, market_id, delivered_at);

-- Hauling events are counted in the traffic statistics too.
ALTER TABLE ingest_stats DROP CONSTRAINT ingest_stats_kind_check;
ALTER TABLE ingest_stats ADD CONSTRAINT ingest_stats_kind_check
    CHECK (kind IN ('carrier', 'market', 'hauling'));
