use serde_json::Value;
use sqlx::{PgConnection, PgPool, QueryBuilder};

use crate::journal_definitions::Commodity;

/// Replace every commodity listed on a carrier's market — the `commodities` half
/// of a `Market` payload.
///
/// A `Market` event is a full snapshot of the market, not a delta, so the old
/// rows are dropped rather than merged. The delete and the insert share the
/// caller's transaction with the market header upsert, which is why this takes a
/// connection rather than a pool.
pub async fn replace(
    conn: &mut PgConnection,
    carrier_id: i64,
    items: &[Commodity],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM market_commodities WHERE carrier_id = $1")
        .bind(carrier_id)
        .execute(&mut *conn)
        .await?;

    // A carrier with nothing listed is a real state; the delete above is the
    // whole write in that case.
    if items.is_empty() {
        return Ok(());
    }

    // One multi-row INSERT rather than a statement per commodity. Carrier markets
    // are small, but a station market runs to a couple of hundred entries and
    // this keeps it to a single round trip.
    let mut qb = QueryBuilder::new(
        "INSERT INTO market_commodities (
            carrier_id, commodity_id, name, name_localised, category,
            category_localised, buy_price, sell_price, mean_price, stock_bracket,
            demand_bracket, stock, demand, consumer, producer, rare) ",
    );

    qb.push_values(items, |mut row, item| {
        row.push_bind(carrier_id)
            .push_bind(item.id)
            .push_bind(&item.name)
            .push_bind(&item.name_localised)
            .push_bind(&item.category)
            .push_bind(&item.category_localised)
            .push_bind(item.buy_price)
            .push_bind(item.sell_price)
            .push_bind(item.mean_price)
            .push_bind(item.stock_bracket)
            .push_bind(item.demand_bracket)
            .push_bind(item.stock)
            .push_bind(item.demand)
            .push_bind(item.consumer)
            .push_bind(item.producer)
            .push_bind(item.rare);
    });

    qb.build().execute(&mut *conn).await?;

    Ok(())
}

/// Every commodity listed on a carrier's market, ordered for display: by
/// category, then name, with the unlocalised entries last.
pub async fn list(pool: &PgPool, carrier_id: i64) -> sqlx::Result<Vec<Value>> {
    sqlx::query_scalar(
        "
        SELECT to_jsonb(c)
        FROM market_commodities c
        WHERE c.carrier_id = $1
        ORDER BY c.category_localised NULLS LAST,
                 c.name_localised NULLS LAST,
                 c.commodity_id
        ",
    )
    .bind(carrier_id)
    .fetch_all(pool)
    .await
}
