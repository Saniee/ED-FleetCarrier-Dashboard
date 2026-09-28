use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};

use crate::journal_definitions::{
    CarrierCrewServices, CarrierEvent, CarrierJump, CarrierTradeOrder, PackOrder,
};

/// The current carrier table serialized as JSON, as published to subscribers.
pub type CarrierSnapshot = Value;

/// Apply one journal event to the `carriers` state table.
///
/// This module owns the `carriers` table exclusively; it never touches
/// `carrier_events`. Returns the `carrier_id` that was written, or `None` when
/// the event could not be attributed to a carrier (an on-foot `CarrierJump`
/// whose system does not match any known carrier).
pub async fn apply(pool: &PgPool, event: &CarrierEvent) -> sqlx::Result<Option<i64>> {
    match event {
        CarrierEvent::CarrierBuy(e) => upsert_buy(pool, e).await.map(Some),
        CarrierEvent::CarrierStats(e) => upsert_stats(pool, e).await.map(Some),
        CarrierEvent::CarrierJumpRequest(e) => upsert_jump_request(pool, e).await.map(Some),
        CarrierEvent::CarrierJumpCancelled(e) => cancel_jump(pool, e).await,
        CarrierEvent::CarrierBankTransfer(e) => upsert_bank_transfer(pool, e).await.map(Some),
        CarrierEvent::CarrierDepositFuel(e) => upsert_fuel(pool, e).await.map(Some),
        CarrierEvent::CarrierCrewServices(e) => crew_services(pool, e).await.map(Some),
        CarrierEvent::CarrierFinance(e) => upsert_finance(pool, e).await.map(Some),
        CarrierEvent::CarrierShipPack(e) => {
            pack_change(pool, e, "ship_packs").await.map(Some)
        }
        CarrierEvent::CarrierModulePack(e) => {
            pack_change(pool, e, "module_packs").await.map(Some)
        }
        CarrierEvent::CarrierTradeOrder(e) => trade_order(pool, e).await.map(Some),
        CarrierEvent::CarrierDockingPermission(e) => {
            upsert_docking_access(pool, e).await.map(Some)
        }
        CarrierEvent::CarrierNameChange(e) => upsert_name(pool, e).await.map(Some),
        CarrierEvent::CarrierLocation(e) => upsert_location(pool, e).await.map(Some),
        CarrierEvent::CarrierJump(j) => apply_jump(pool, j).await,
    }
}

/// Record which history row produced the current carrier state.
pub async fn set_last_event(
    pool: &PgPool,
    carrier_id: i64,
    event_id: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE carriers SET last_event_id = $2, updated_at = now() WHERE carrier_id = $1",
    )
    .bind(carrier_id)
    .bind(event_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// The full carrier row as JSON, for the API response.
pub async fn get(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Option<CarrierSnapshot>> {
    sqlx::query_scalar("SELECT to_jsonb(c) FROM carriers c WHERE carrier_id = $1")
        .bind(carrier_id)
        .fetch_optional(pool)
        .await
}

/// The most recently updated carrier, for single-tenant reads and as the
/// initial payload sent to a new stream subscriber.
pub async fn get_current(pool: &PgPool) -> sqlx::Result<Option<CarrierSnapshot>> {
    sqlx::query_scalar("SELECT to_jsonb(c) FROM carriers c ORDER BY updated_at DESC LIMIT 1")
        .fetch_optional(pool)
        .await
}

// ---------------------------------------------------------------------------
// Identity / purchase / name
// ---------------------------------------------------------------------------

async fn upsert_buy(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierBuy,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, callsign, variant, purchase_price, purchased_at,
                              bought_at_market, purchase_location, purchase_system_address,
                              updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            callsign = EXCLUDED.callsign,
            variant = EXCLUDED.variant,
            purchase_price = EXCLUDED.purchase_price,
            purchased_at = EXCLUDED.purchased_at,
            bought_at_market = EXCLUDED.bought_at_market,
            purchase_location = EXCLUDED.purchase_location,
            purchase_system_address = EXCLUDED.purchase_system_address,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.callsign)
    .bind(&e.variant)
    .bind(e.price)
    .bind(&e.timestamp)
    .bind(e.bought_at_market)
    .bind(&e.location)
    .bind(e.system_address)
    .fetch_one(pool)
    .await
}

async fn upsert_name(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierNameChange,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, callsign, name, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            callsign = EXCLUDED.callsign,
            name = EXCLUDED.name,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.callsign)
    .bind(&e.name)
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// Stats block (identity, access, fuel, space, finance, collections)
// ---------------------------------------------------------------------------

async fn upsert_stats(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierStats,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (
            carrier_id, callsign, name, docking_access, allow_notorious,
            fuel_level, jump_range_curr, jump_range_max,
            space_total_capacity, space_crew, space_cargo, space_cargo_reserved,
            space_ship_packs, space_module_packs, space_free,
            carrier_balance, reserve_balance, available_balance, reserve_percent,
            crew, ship_packs, module_packs, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21, $22, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            callsign = EXCLUDED.callsign,
            name = EXCLUDED.name,
            docking_access = EXCLUDED.docking_access,
            allow_notorious = EXCLUDED.allow_notorious,
            fuel_level = EXCLUDED.fuel_level,
            jump_range_curr = EXCLUDED.jump_range_curr,
            jump_range_max = EXCLUDED.jump_range_max,
            space_total_capacity = EXCLUDED.space_total_capacity,
            space_crew = EXCLUDED.space_crew,
            space_cargo = EXCLUDED.space_cargo,
            space_cargo_reserved = EXCLUDED.space_cargo_reserved,
            space_ship_packs = EXCLUDED.space_ship_packs,
            space_module_packs = EXCLUDED.space_module_packs,
            space_free = EXCLUDED.space_free,
            carrier_balance = EXCLUDED.carrier_balance,
            reserve_balance = EXCLUDED.reserve_balance,
            available_balance = EXCLUDED.available_balance,
            reserve_percent = EXCLUDED.reserve_percent,
            crew = EXCLUDED.crew,
            ship_packs = EXCLUDED.ship_packs,
            module_packs = EXCLUDED.module_packs,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.callsign)
    .bind(&e.name)
    .bind(&e.docking_access)
    .bind(e.allow_notorious)
    .bind(e.fuel_level)
    .bind(e.jump_range_curr)
    .bind(e.jump_range_max)
    .bind(e.space_usage.total_capacity)
    .bind(e.space_usage.crew)
    .bind(e.space_usage.cargo)
    .bind(e.space_usage.cargo_space_reserved)
    .bind(e.space_usage.ship_packs)
    .bind(e.space_usage.module_packs)
    .bind(e.space_usage.free_space)
    .bind(e.finance.carrier_balance)
    .bind(e.finance.reserve_balance)
    .bind(e.finance.available_balance)
    .bind(e.finance.reserve_percent)
    .bind(Json(&e.crew))
    .bind(Json(&e.ship_packs))
    .bind(Json(&e.module_packs))
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// Jump scheduling
// ---------------------------------------------------------------------------

async fn upsert_jump_request(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierJumpRequest,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, jump_destination_system, jump_destination_system_address,
                              jump_destination_body, jump_destination_body_id, jump_departure_time,
                              updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            jump_destination_system = EXCLUDED.jump_destination_system,
            jump_destination_system_address = EXCLUDED.jump_destination_system_address,
            jump_destination_body = EXCLUDED.jump_destination_body,
            jump_destination_body_id = EXCLUDED.jump_destination_body_id,
            jump_departure_time = EXCLUDED.jump_departure_time,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.system_name)
    .bind(e.system_address)
    .bind(&e.body)
    .bind(e.body_id)
    .bind(&e.departure_time)
    .fetch_one(pool)
    .await
}

async fn cancel_jump(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierJumpCancelled,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "
        UPDATE carriers SET
            jump_destination_system = NULL,
            jump_destination_system_address = NULL,
            jump_destination_body = NULL,
            jump_destination_body_id = NULL,
            jump_departure_time = NULL,
            updated_at = now()
        WHERE carrier_id = $1
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .fetch_optional(pool)
    .await
}

// ---------------------------------------------------------------------------
// Money and fuel
// ---------------------------------------------------------------------------

async fn upsert_bank_transfer(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierBankTransfer,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, carrier_balance, updated_at)
        VALUES ($1, $2, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            carrier_balance = EXCLUDED.carrier_balance,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(e.carrier_balance)
    .fetch_one(pool)
    .await
}

async fn upsert_fuel(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierDepositFuel,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, fuel_level, updated_at)
        VALUES ($1, $2, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            fuel_level = EXCLUDED.fuel_level,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(e.total)
    .fetch_one(pool)
    .await
}

async fn upsert_finance(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierFinance,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, carrier_balance, reserve_balance, available_balance,
                              reserve_percent, tax_rate_shipyard, tax_rate_rearm,
                              tax_rate_outfitting, tax_rate_refuel, tax_rate_repair, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            carrier_balance = EXCLUDED.carrier_balance,
            reserve_balance = EXCLUDED.reserve_balance,
            available_balance = EXCLUDED.available_balance,
            reserve_percent = EXCLUDED.reserve_percent,
            tax_rate_shipyard = EXCLUDED.tax_rate_shipyard,
            tax_rate_rearm = EXCLUDED.tax_rate_rearm,
            tax_rate_outfitting = EXCLUDED.tax_rate_outfitting,
            tax_rate_refuel = EXCLUDED.tax_rate_refuel,
            tax_rate_repair = EXCLUDED.tax_rate_repair,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(e.carrier_balance)
    .bind(e.reserve_balance)
    .bind(e.available_balance)
    .bind(e.reserve_percent)
    .bind(e.tax_rate_shipyard)
    .bind(e.tax_rate_rearm)
    .bind(e.tax_rate_outfitting)
    .bind(e.tax_rate_refuel)
    .bind(e.tax_rate_repair)
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// Access
// ---------------------------------------------------------------------------

async fn upsert_docking_access(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierDockingPermission,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, docking_access, allow_notorious, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            docking_access = EXCLUDED.docking_access,
            allow_notorious = EXCLUDED.allow_notorious,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.docking_access)
    .bind(e.allow_notorious)
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

async fn upsert_location(
    pool: &PgPool,
    e: &crate::journal_definitions::CarrierLocation,
) -> sqlx::Result<i64> {
    // `market_id` is the carrier's MarketID; for fleet carriers that is the
    // same number as `carrier_id`, so it is set from the event's CarrierID.
    sqlx::query_scalar(
        "
        INSERT INTO carriers (carrier_id, market_id, star_system, system_address, body_id, updated_at)
        VALUES ($1, $1, $2, $3, $4, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            market_id = EXCLUDED.market_id,
            star_system = EXCLUDED.star_system,
            system_address = EXCLUDED.system_address,
            body_id = EXCLUDED.body_id,
            updated_at = now()
        RETURNING carrier_id
        ",
    )
    .bind(e.carrier_id)
    .bind(&e.star_system)
    .bind(e.system_address)
    .bind(e.body_id)
    .fetch_one(pool)
    .await
}

/// `CarrierJump` arrives in two shapes:
///
/// * docked — carries `MarketID` (equal to the carrier's `CarrierID`) and the
///   full station block. Identified directly and upserted.
/// * on foot — omits `MarketID` and the station block. It is attributed to the
///   carrier whose current `system_address` matches. This assumes at most one
///   tracked carrier sits in a given system; a multi-carrier deployment sharing
///   a system would need a stronger link.
async fn apply_jump(pool: &PgPool, j: &CarrierJump) -> sqlx::Result<Option<i64>> {
    let star_x = j.star_pos.first().copied();
    let star_y = j.star_pos.get(1).copied();
    let star_z = j.star_pos.get(2).copied();

    match j.market_id {
        Some(market_id) => {
            let id = sqlx::query_scalar(
                "
                INSERT INTO carriers (
                    carrier_id, market_id, docked,
                    station_name, station_type, station_faction, station_government,
                    station_government_localised, station_services, station_economy,
                    station_economy_localised, station_economies,
                    star_system, system_address, star_x, star_y, star_z,
                    system_allegiance, system_economy, system_economy_localised,
                    system_second_economy, system_second_economy_localised,
                    system_government, system_government_localised,
                    system_security, system_security_localised,
                    population, body, body_id, body_type, system_faction, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                        $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29,
                        $30, $31, now())
                ON CONFLICT (carrier_id) DO UPDATE SET
                    market_id = EXCLUDED.market_id,
                    docked = EXCLUDED.docked,
                    station_name = EXCLUDED.station_name,
                    station_type = EXCLUDED.station_type,
                    station_faction = EXCLUDED.station_faction,
                    station_government = EXCLUDED.station_government,
                    station_government_localised = EXCLUDED.station_government_localised,
                    station_services = EXCLUDED.station_services,
                    station_economy = EXCLUDED.station_economy,
                    station_economy_localised = EXCLUDED.station_economy_localised,
                    station_economies = EXCLUDED.station_economies,
                    star_system = EXCLUDED.star_system,
                    system_address = EXCLUDED.system_address,
                    star_x = EXCLUDED.star_x,
                    star_y = EXCLUDED.star_y,
                    star_z = EXCLUDED.star_z,
                    system_allegiance = EXCLUDED.system_allegiance,
                    system_economy = EXCLUDED.system_economy,
                    system_economy_localised = EXCLUDED.system_economy_localised,
                    system_second_economy = EXCLUDED.system_second_economy,
                    system_second_economy_localised = EXCLUDED.system_second_economy_localised,
                    system_government = EXCLUDED.system_government,
                    system_government_localised = EXCLUDED.system_government_localised,
                    system_security = EXCLUDED.system_security,
                    system_security_localised = EXCLUDED.system_security_localised,
                    population = EXCLUDED.population,
                    body = EXCLUDED.body,
                    body_id = EXCLUDED.body_id,
                    body_type = EXCLUDED.body_type,
                    system_faction = EXCLUDED.system_faction,
                    updated_at = now()
                RETURNING carrier_id
                ",
            )
            .bind(market_id)
            .bind(market_id)
            .bind(j.docked)
            .bind(&j.station_name)
            .bind(&j.station_type)
            .bind(j.station_faction.as_ref().map(|f| f.name.as_str()))
            .bind(&j.station_government)
            .bind(&j.station_government_localised)
            .bind(&j.station_services)
            .bind(&j.station_economy)
            .bind(&j.station_economy_localised)
            .bind(j.station_economies.as_ref().map(Json))
            .bind(&j.star_system)
            .bind(j.system_address)
            .bind(star_x)
            .bind(star_y)
            .bind(star_z)
            .bind(&j.system_allegiance)
            .bind(&j.system_economy)
            .bind(&j.system_economy_localised)
            .bind(&j.system_second_economy)
            .bind(&j.system_second_economy_localised)
            .bind(&j.system_government)
            .bind(&j.system_government_localised)
            .bind(&j.system_security)
            .bind(&j.system_security_localised)
            .bind(j.population)
            .bind(&j.body)
            .bind(j.body_id)
            .bind(&j.body_type)
            .bind(j.system_faction.as_ref().map(|f| f.name.as_str()))
            .fetch_one(pool)
            .await?;

            Ok(Some(id))
        }
        None => {
            let id = sqlx::query_scalar(
                "
                UPDATE carriers SET
                    docked = $2,
                    star_system = $3,
                    system_address = $4,
                    star_x = $5, star_y = $6, star_z = $7,
                    system_allegiance = $8, system_economy = $9,
                    system_economy_localised = $10,
                    system_second_economy = $11, system_second_economy_localised = $12,
                    system_government = $13, system_government_localised = $14,
                    system_security = $15, system_security_localised = $16,
                    population = $17, body = $18, body_id = $19, body_type = $20,
                    system_faction = $21, updated_at = now()
                WHERE carrier_id = (
                    SELECT carrier_id FROM carriers
                    WHERE system_address = $1
                    ORDER BY updated_at DESC
                    LIMIT 1
                )
                RETURNING carrier_id
                ",
            )
            .bind(j.system_address)
            .bind(j.docked)
            .bind(&j.star_system)
            .bind(j.system_address)
            .bind(star_x)
            .bind(star_y)
            .bind(star_z)
            .bind(&j.system_allegiance)
            .bind(&j.system_economy)
            .bind(&j.system_economy_localised)
            .bind(&j.system_second_economy)
            .bind(&j.system_second_economy_localised)
            .bind(&j.system_government)
            .bind(&j.system_government_localised)
            .bind(&j.system_security)
            .bind(&j.system_security_localised)
            .bind(j.population)
            .bind(&j.body)
            .bind(j.body_id)
            .bind(&j.body_type)
            .bind(j.system_faction.as_ref().map(|f| f.name.as_str()))
            .fetch_optional(pool)
            .await?;

            Ok(id)
        }
    }
}

// ---------------------------------------------------------------------------
// Collections stored as jsonb
// ---------------------------------------------------------------------------

/// Read a jsonb column for a carrier, defaulting to the supplied value when the
/// row is missing or the column is NULL.
async fn read_jsonb(
    pool: &PgPool,
    carrier_id: i64,
    column: &str,
    default: Value,
) -> sqlx::Result<(bool, Value)> {
    // `column` is one of a fixed set of internal literals, never caller input,
    // so the dynamic SQL is safe to assert.
    let sql = format!("SELECT {column} FROM carriers WHERE carrier_id = $1");
    let row: Option<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(carrier_id)
        .fetch_optional(pool)
        .await?;

    match row {
        // Row exists but the column is NULL.
        Some(Value::Null) => Ok((true, default)),
        Some(value) => Ok((true, value)),
        None => Ok((false, default)),
    }
}

/// Write a jsonb column, creating the carrier row if it does not exist yet.
async fn write_jsonb(
    pool: &PgPool,
    carrier_id: i64,
    column: &str,
    value: &Value,
) -> sqlx::Result<i64> {
    let sql = format!(
        "
        INSERT INTO carriers (carrier_id, {column}, updated_at)
        VALUES ($1, $2, now())
        ON CONFLICT (carrier_id) DO UPDATE SET
            {column} = EXCLUDED.{column},
            updated_at = now()
        RETURNING carrier_id
        "
    );

    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(carrier_id)
        .bind(Json(value))
        .fetch_one(pool)
        .await
}

async fn crew_services(pool: &PgPool, e: &CarrierCrewServices) -> sqlx::Result<i64> {
    let (_, mut crew) = read_jsonb(
        pool,
        e.carrier_id,
        "crew",
        Value::Array(Vec::new()),
    )
    .await?;

    if !crew.is_array() {
        crew = Value::Array(Vec::new());
    }
    let entries = crew.as_array_mut().expect("checked above");

    let existing = entries
        .iter_mut()
        .find(|c| c.get("CrewRole").and_then(Value::as_str) == Some(e.crew_role.as_str()));

    match existing {
        Some(entry) => match e.operation.as_str() {
            "Activate" => {
                entry["Activated"] = json!(true);
                entry["Enabled"] = json!(true);
                entry["CrewName"] = json!(e.crew_name);
            }
            "Deactivate" => {
                entry["Activated"] = json!(false);
                if let Value::Object(map) = entry {
                    map.remove("Enabled");
                    map.remove("CrewName");
                }
            }
            "Pause" => entry["Enabled"] = json!(false),
            "Resume" => entry["Enabled"] = json!(true),
            "Replace" => entry["CrewName"] = json!(e.crew_name),
            _ => {}
        },
        None => {
            if e.operation == "Activate" {
                entries.push(json!({
                    "CrewRole": e.crew_role,
                    "Activated": true,
                    "Enabled": true,
                    "CrewName": e.crew_name,
                }));
            }
        }
    }

    write_jsonb(pool, e.carrier_id, "crew", &crew).await
}

async fn pack_change(pool: &PgPool, e: &PackOrder, column: &str) -> sqlx::Result<i64> {
    let (_, mut packs) = read_jsonb(pool, e.carrier_id, column, Value::Array(Vec::new())).await?;

    if !packs.is_array() {
        packs = Value::Array(Vec::new());
    }
    let entries = packs.as_array_mut().expect("checked above");

    let index = entries
        .iter()
        .position(|p| p.get("PackTheme").and_then(Value::as_str) == Some(e.pack_theme.as_str()));

    match e.operation.as_str() {
        "SellPack" => {
            if let Some(i) = index {
                entries.remove(i);
            }
        }
        // BuyPack / RestockPack both install or update the pack.
        _ => match index {
            Some(i) => entries[i]["PackTier"] = json!(e.pack_tier),
            None => entries.push(json!({
                "PackTheme": e.pack_theme,
                "PackTier": e.pack_tier,
            })),
        },
    }

    write_jsonb(pool, e.carrier_id, column, &packs).await
}

async fn trade_order(pool: &PgPool, e: &CarrierTradeOrder) -> sqlx::Result<i64> {
    let (_, mut orders) = read_jsonb(pool, e.carrier_id, "trade_orders", json!({})).await?;

    if !orders.is_object() {
        orders = json!({});
    }
    let map = orders.as_object_mut().expect("checked above");

    if e.cancel_trade == Some(true) {
        map.remove(&e.commodity);
    } else {
        let mut entry = json!({
            "BlackMarket": e.black_market,
        });
        if let Some(price) = e.price {
            entry["Price"] = json!(price);
        }
        if let Some(purchase) = e.purchase_order {
            entry["PurchaseOrder"] = json!(purchase);
        }
        if let Some(sale) = e.sale_order {
            entry["SaleOrder"] = json!(sale);
        }
        map.insert(e.commodity.clone(), entry);
    }

    write_jsonb(pool, e.carrier_id, "trade_orders", &orders).await
}
