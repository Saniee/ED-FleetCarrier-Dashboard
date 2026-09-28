-- Event history: every raw journal event, in order.
CREATE TABLE carrier_events (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    timestamp     timestamptz NOT NULL,
    event_name    text NOT NULL,
    json_data     jsonb NOT NULL
);

CREATE INDEX carrier_events_name_timestamp_idx ON carrier_events (event_name, timestamp);

-- One row per fleet carrier, holding its latest known state.
-- Every journal event upserts into this row; columns are nullable because a
-- carrier's row is filled in gradually as different events arrive.

CREATE TABLE carriers (
    -- Identity (CarrierBuy, CarrierStats, CarrierNameChanged)
    carrier_id                          bigint PRIMARY KEY,         -- CarrierID
    callsign                            text UNIQUE,
    name                                text,
    market_id                           bigint UNIQUE,              -- MarketID (from CarrierJump)

    -- Purchase (CarrierBuy)
    variant                             text,
    purchase_price                      bigint,
    purchased_at                        timestamptz,
    bought_at_market                    bigint,
    purchase_location                   text,
    purchase_system_address             bigint,

    -- Access (CarrierStats, CarrierDockingPermission)
    docking_access                      text,
    allow_notorious                     boolean,

    -- Fuel and range (CarrierStats, CarrierDepositFuel)
    fuel_level                          bigint,
    jump_range_curr                     double precision,
    jump_range_max                      double precision,

    -- Space usage (CarrierStats.SpaceUsage)
    space_total_capacity                bigint,
    space_crew                          bigint,
    space_cargo                         bigint,
    space_cargo_reserved                bigint,
    space_ship_packs                    bigint,
    space_module_packs                  bigint,
    space_free                          bigint,

    -- Finance (CarrierStats.Finance, CarrierFinance, CarrierBankTransfer)
    carrier_balance                     bigint,
    reserve_balance                     bigint,
    available_balance                   bigint,
    reserve_percent                     bigint,
    tax_rate                            bigint,

    -- Current location (CarrierJump)
    docked                              boolean,
    star_system                         text,
    system_address                      bigint,
    star_x                              double precision,           -- StarPos[0]
    star_y                              double precision,           -- StarPos[1]
    star_z                              double precision,           -- StarPos[2]
    body                                text,
    body_id                             bigint,
    body_type                           text,
    population                          bigint,

    -- Current system (CarrierJump)
    system_allegiance                   text,
    system_faction                      text,                       -- Faction.name
    system_economy                      text,
    system_economy_localised            text,
    system_second_economy               text,
    system_second_economy_localised     text,
    system_government                   text,
    system_government_localised         text,
    system_security                     text,
    system_security_localised           text,

    -- Carrier's own station info (CarrierJump)
    station_name                        text,
    station_type                        text,
    station_faction                     text,                       -- Faction.name
    station_government                  text,
    station_government_localised        text,
    station_economy                     text,
    station_economy_localised           text,
    station_services                    text[],
    station_economies                   jsonb,                      -- [{Name, Name_Localised, Proportion}]

    -- Scheduled jump (CarrierJumpRequest); set all NULL on CarrierJumpCancelled
    jump_destination_system             text,                       -- SystemName
    jump_destination_system_address     bigint,
    jump_destination_body               text,                       -- absent when no body
    jump_destination_body_id            bigint,
    jump_departure_time                 timestamptz,

    -- Nested collections (CarrierStats, CarrierCrewServices, pack events, CarrierTradeOrder)
    crew                                jsonb,                      -- [{CrewRole, Activated, Enabled, CrewName}]
    ship_packs                          jsonb,                      -- [{PackTheme, PackTier}]
    module_packs                        jsonb,                      -- [{PackTheme, PackTier}]
    trade_orders                        jsonb,                      -- keyed by commodity

    -- Bookkeeping
    last_event_id                       bigint REFERENCES carrier_events(id),  -- last event row applied to this carrier
    updated_at                          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX carriers_system_address_idx ON carriers (system_address);