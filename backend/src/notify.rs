//! What changed on a carrier that its owner's Discord webhook should hear about.
//!
//! Notices come from comparing the carrier before and after an event, not from
//! the event alone: the plugin resends events (journal replays, several
//! commanders docked at one carrier), and a repeat changes nothing, so it is
//! announced once.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use crate::journal_definitions::{CarrierEvent, Commodity};

/// The notice kinds an owner can switch on, as stored in `carrier_webhooks.events`.
pub const EVENTS: [&str; 5] = ["jump", "arrival", "trade_order", "order_filled", "low_fuel"];

/// Events older than this are replays: they may still change the carrier, but
/// announcing them would be news from the past.
const STALE_AFTER: Duration = Duration::minutes(30);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    JumpScheduled { system: String, body: Option<String>, departure: String },
    JumpCancelled { system: String },
    Arrived { system: String, body: Option<String> },
    OrderPosted { commodity: String, side: Side, quantity: i64, price: Option<i64> },
    OrderCancelled { commodity: String },
    OrderFilled { commodity: String, side: Side },
    LowFuel { level: i64, threshold: i64 },
}

impl Notice {
    /// The `EVENTS` entry that switches this notice on.
    pub fn event(&self) -> &'static str {
        match self {
            Notice::JumpScheduled { .. } | Notice::JumpCancelled { .. } => "jump",
            Notice::Arrived { .. } => "arrival",
            Notice::OrderPosted { .. } | Notice::OrderCancelled { .. } => "trade_order",
            Notice::OrderFilled { .. } => "order_filled",
            Notice::LowFuel { .. } => "low_fuel",
        }
    }
}

/// Whether a journal timestamp is recent enough to announce. Unparseable ones
/// are treated as stale: better a missed notice than a wrong one.
pub fn is_fresh(timestamp: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(timestamp)
        .is_ok_and(|t| now.signed_duration_since(t) < STALE_AFTER)
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

fn number(row: &Value, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

/// Notices for one carrier event, from the `carriers` row before and after it
/// was applied. `low_fuel` is the owner's threshold in tonnes; 0 is off.
pub fn carrier_notices(event: &CarrierEvent, before: &Value, after: &Value, low_fuel: i64) -> Vec<Notice> {
    let mut out = Vec::new();

    match event {
        CarrierEvent::CarrierJumpRequest(e) => {
            let unchanged = text(before, "jump_destination_system") == Some(e.system_name.as_str())
                && text(before, "jump_departure_time") == Some(e.departure_time.as_str());
            if !unchanged {
                out.push(Notice::JumpScheduled {
                    system: e.system_name.clone(),
                    body: e.body.clone(),
                    departure: e.departure_time.clone(),
                });
            }
        }
        CarrierEvent::CarrierJumpCancelled(_) => {
            if let Some(system) = text(before, "jump_destination_system") {
                out.push(Notice::JumpCancelled { system: system.to_string() });
            }
        }
        CarrierEvent::CarrierJump(_) | CarrierEvent::CarrierLocation(_) => {
            // A carrier seen for the first time has no "before" system: that is
            // not an arrival.
            if let (Some(from), Some(to)) = (text(before, "star_system"), text(after, "star_system"))
                && from != to
            {
                out.push(Notice::Arrived {
                    system: to.to_string(),
                    body: text(after, "body").map(str::to_string),
                });
            }
        }
        CarrierEvent::CarrierTradeOrder(e) => {
            let commodity = e.commodity_localised.clone().unwrap_or_else(|| e.commodity.clone());
            let order = |row: &Value| row.get("trade_orders").and_then(|o| o.get(&e.commodity)).cloned();
            let (was, now) = (order(before), order(after));

            if e.cancel_trade == Some(true) {
                if was.is_some() {
                    out.push(Notice::OrderCancelled { commodity });
                }
            } else if was != now {
                let (side, quantity) = match (e.purchase_order, e.sale_order) {
                    (Some(q), _) => (Side::Buy, q),
                    (None, Some(q)) => (Side::Sell, q),
                    (None, None) => return out,
                };
                out.push(Notice::OrderPosted { commodity, side, quantity, price: e.price });
            }
        }
        _ => {}
    }

    if low_fuel > 0
        && let (Some(was), Some(now)) = (number(before, "fuel_level"), number(after, "fuel_level"))
        && was >= low_fuel
        && now < low_fuel
    {
        out.push(Notice::LowFuel { level: now, threshold: low_fuel });
    }

    out
}

/// `iridium` from the market's `$iridium_name;`: the key trade orders use.
fn commodity_key(symbol: &str) -> String {
    symbol
        .trim_start_matches('$')
        .trim_end_matches(';')
        .trim_end_matches("_name")
        .to_ascii_lowercase()
}

/// Orders that ran out between two reads of a carrier's market: a buy order
/// whose demand reached zero, or a sale whose stock did. `before` holds the
/// stored `market_commodities` rows, `orders` the carrier's `trade_orders`, so a
/// commodity the owner cancelled in between doesn't count as filled.
pub fn market_notices(before: &[Value], after: &[Commodity], orders: &Value) -> Vec<Notice> {
    // A market read without its commodity list says nothing about the orders.
    if after.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for row in before {
        let Some(symbol) = text(row, "name") else { continue };
        let key = commodity_key(symbol);
        let Some(order) = orders
            .as_object()
            .and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key)))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let now = after.iter().find(|c| Some(c.id) == number(row, "commodity_id"));
        let commodity = text(row, "name_localised").unwrap_or(&key).to_string();
        let positive = |k: &str| number(row, k).is_some_and(|n| n > 0);

        if order.get("PurchaseOrder").is_some()
            && positive("buy_price")
            && positive("demand")
            && now.is_none_or(|c| c.demand == 0)
        {
            out.push(Notice::OrderFilled { commodity: commodity.clone(), side: Side::Buy });
        }
        if order.get("SaleOrder").is_some()
            && positive("sell_price")
            && positive("stock")
            && now.is_none_or(|c| c.stock == 0)
        {
            out.push(Notice::OrderFilled { commodity, side: Side::Sell });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(line: Value) -> CarrierEvent {
        serde_json::from_value(line).unwrap()
    }

    fn jump_request(system: &str, departure: &str) -> CarrierEvent {
        event(json!({
            "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierJumpRequest",
            "CarrierID": 1, "SystemName": system, "Body": "Body A", "SystemAddress": 2,
            "BodyID": 3, "DepartureTime": departure
        }))
    }

    #[test]
    fn jump_request_is_announced_once() {
        let req = jump_request("Sol", "2026-10-27T12:15:00Z");
        let after = json!({ "jump_destination_system": "Sol", "jump_departure_time": "2026-10-27T12:15:00Z" });

        let first = carrier_notices(&req, &json!({}), &after, 0);
        assert!(matches!(&first[..], [Notice::JumpScheduled { system, .. }] if system == "Sol"));
        assert!(carrier_notices(&req, &after, &after, 0).is_empty());
    }

    #[test]
    fn cancel_needs_a_pending_jump() {
        let cancel = event(json!({ "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierJumpCancelled", "CarrierID": 1 }));
        let pending = json!({ "jump_destination_system": "Sol" });

        assert_eq!(
            carrier_notices(&cancel, &pending, &json!({}), 0),
            vec![Notice::JumpCancelled { system: "Sol".into() }]
        );
        assert!(carrier_notices(&cancel, &json!({}), &json!({}), 0).is_empty());
    }

    #[test]
    fn arrival_only_when_the_system_changes() {
        let loc = event(json!({
            "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierLocation", "CarrierID": 1,
            "StarSystem": "Sol", "SystemAddress": 2, "BodyID": 3
        }));
        let sol = json!({ "star_system": "Sol", "body": "Earth" });

        assert_eq!(
            carrier_notices(&loc, &json!({ "star_system": "Alpha Centauri" }), &sol, 0),
            vec![Notice::Arrived { system: "Sol".into(), body: Some("Earth".into()) }]
        );
        assert!(carrier_notices(&loc, &sol, &sol, 0).is_empty());
        // First sighting.
        assert!(carrier_notices(&loc, &json!({ "star_system": null }), &sol, 0).is_empty());
    }

    #[test]
    fn trade_orders() {
        let buy = event(json!({
            "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierTradeOrder", "CarrierID": 1,
            "BlackMarket": false, "Commodity": "steel", "Commodity_Localised": "Steel",
            "PurchaseOrder": 500, "Price": 4000
        }));
        let with = json!({ "trade_orders": { "steel": { "PurchaseOrder": 500, "Price": 4000 } } });

        assert_eq!(
            carrier_notices(&buy, &json!({ "trade_orders": {} }), &with, 0),
            vec![Notice::OrderPosted { commodity: "Steel".into(), side: Side::Buy, quantity: 500, price: Some(4000) }]
        );
        assert!(carrier_notices(&buy, &with, &with, 0).is_empty());

        let cancel = event(json!({
            "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierTradeOrder", "CarrierID": 1,
            "BlackMarket": false, "Commodity": "steel", "CancelTrade": true
        }));
        assert_eq!(
            carrier_notices(&cancel, &with, &json!({ "trade_orders": {} }), 0),
            vec![Notice::OrderCancelled { commodity: "steel".into() }]
        );
        assert!(carrier_notices(&cancel, &json!({ "trade_orders": {} }), &json!({}), 0).is_empty());
    }

    #[test]
    fn low_fuel_fires_when_crossing_the_threshold() {
        let deposit = event(json!({
            "timestamp": "2026-10-27T12:00:00Z", "event": "CarrierDepositFuel", "CarrierID": 1,
            "Amount": 1, "Total": 150
        }));
        let low = |was: i64, now: i64, t: i64| {
            carrier_notices(&deposit, &json!({ "fuel_level": was }), &json!({ "fuel_level": now }), t)
        };

        assert_eq!(low(250, 150, 200), vec![Notice::LowFuel { level: 150, threshold: 200 }]);
        assert!(low(180, 150, 200).is_empty(), "already below");
        assert!(low(250, 210, 200).is_empty(), "still above");
        assert!(low(250, 150, 0).is_empty(), "switched off");
    }

    fn commodity(id: i64, stock: i64, demand: i64) -> Commodity {
        serde_json::from_value(json!({ "id": id, "Name": "$steel_name;", "Stock": stock, "Demand": demand })).unwrap()
    }

    #[test]
    fn buy_order_filled() {
        let before = [json!({ "commodity_id": 7, "name": "$steel_name;", "name_localised": "Steel",
                              "buy_price": 4000, "sell_price": 0, "stock": 0, "demand": 20 })];
        let orders = json!({ "steel": { "PurchaseOrder": 500 } });

        assert_eq!(
            market_notices(&before, &[commodity(7, 0, 0)], &orders),
            vec![Notice::OrderFilled { commodity: "Steel".into(), side: Side::Buy }]
        );
        assert!(market_notices(&before, &[commodity(7, 0, 5)], &orders).is_empty(), "still open");
        assert!(market_notices(&before, &[commodity(7, 0, 0)], &json!({})).is_empty(), "cancelled");
        assert!(market_notices(&before, &[], &orders).is_empty(), "no list");
    }

    #[test]
    fn freshness() {
        let now = DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z").unwrap().to_utc();
        assert!(is_fresh("2026-10-27T11:50:00Z", now));
        assert!(!is_fresh("2026-10-27T11:00:00Z", now));
        assert!(!is_fresh("garbage", now));
    }
}
