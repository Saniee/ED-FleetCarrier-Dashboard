//! A hauling operation's progress, worked out from its targets and the
//! deliveries reported since it started. Pure, so the arithmetic is tested
//! without a database; `db::operations` gathers the inputs.

use std::collections::HashMap;

use serde::Serialize;

/// One commodity the operation needs. `provided` is the depot's own count for a
/// colonisation operation, `None` otherwise (then it is what was sold to the
/// carrier).
pub struct Target {
    pub commodity: String,
    pub name: String,
    pub required: i64,
    pub provided: Option<i64>,
}

/// Deliveries summed per account, kind and commodity.
pub struct Delivered {
    pub user_id: i64,
    pub username: String,
    /// `carrier` or `depot`
    pub kind: String,
    pub commodity: String,
    pub amount: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct TargetProgress {
    pub commodity: String,
    pub name: String,
    pub required: i64,
    /// Counted towards the target: the depot's figure, or sales to the carrier.
    pub provided: i64,
    pub remaining: i64,
    /// Waiting in the carrier's market, from its last market read.
    pub on_carrier: i64,
    /// Sold to the carrier / handed to the depot since the operation started,
    /// by commanders whose plugin reported it.
    pub to_carrier: i64,
    pub to_depot: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Contributor {
    pub user_id: i64,
    pub username: String,
    pub to_carrier: i64,
    pub to_depot: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Progress {
    pub targets: Vec<TargetProgress>,
    pub contributors: Vec<Contributor>,
    pub required: i64,
    pub provided: i64,
    /// 0..1
    pub fraction: f64,
    /// Every target met. Never true without targets.
    pub met: bool,
}

pub fn compute(targets: Vec<Target>, stock: &HashMap<String, i64>, delivered: &[Delivered]) -> Progress {
    let sum = |commodity: &str, kind: &str| -> i64 {
        delivered
            .iter()
            .filter(|d| d.commodity == commodity && d.kind == kind)
            .map(|d| d.amount)
            .sum()
    };

    let rows: Vec<TargetProgress> = targets
        .into_iter()
        .map(|t| {
            let to_carrier = sum(&t.commodity, "carrier");
            let to_depot = sum(&t.commodity, "depot");
            let provided = t.provided.unwrap_or(to_carrier).clamp(0, t.required);
            TargetProgress {
                on_carrier: stock.get(&t.commodity).copied().unwrap_or(0),
                remaining: t.required - provided,
                required: t.required,
                provided,
                to_carrier,
                to_depot,
                commodity: t.commodity,
                name: t.name,
            }
        })
        .collect();

    // Only deliveries of what the operation needs count for the leaderboard.
    let mut by_user: HashMap<i64, Contributor> = HashMap::new();
    for d in delivered.iter().filter(|d| rows.iter().any(|t| t.commodity == d.commodity)) {
        let c = by_user.entry(d.user_id).or_insert_with(|| Contributor {
            user_id: d.user_id,
            username: d.username.clone(),
            to_carrier: 0,
            to_depot: 0,
        });
        match d.kind.as_str() {
            "carrier" => c.to_carrier += d.amount,
            _ => c.to_depot += d.amount,
        }
    }
    let mut contributors: Vec<Contributor> = by_user.into_values().collect();
    contributors.sort_by(|a, b| {
        (b.to_carrier + b.to_depot)
            .cmp(&(a.to_carrier + a.to_depot))
            .then_with(|| a.username.cmp(&b.username))
    });

    let required: i64 = rows.iter().map(|t| t.required).sum();
    let provided: i64 = rows.iter().map(|t| t.provided).sum();
    Progress {
        met: !rows.is_empty() && rows.iter().all(|t| t.remaining == 0),
        fraction: if required > 0 { provided as f64 / required as f64 } else { 0.0 },
        targets: rows,
        contributors,
        required,
        provided,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(commodity: &str, required: i64, provided: Option<i64>) -> Target {
        Target { commodity: commodity.into(), name: commodity.into(), required, provided }
    }

    fn delivered(user_id: i64, kind: &str, commodity: &str, amount: i64) -> Delivered {
        Delivered { user_id, username: format!("u{user_id}"), kind: kind.into(), commodity: commodity.into(), amount }
    }

    #[test]
    fn manual_targets_count_sales_to_the_carrier() {
        let stock = HashMap::from([("steel".to_string(), 300)]);
        let p = compute(
            vec![target("steel", 1000, None), target("aluminium", 500, None)],
            &stock,
            &[
                delivered(1, "carrier", "steel", 600),
                delivered(2, "carrier", "steel", 600),
                delivered(2, "carrier", "gold", 50),
                delivered(1, "depot", "aluminium", 100),
            ],
        );

        let steel = &p.targets[0];
        assert_eq!((steel.provided, steel.remaining, steel.on_carrier, steel.to_carrier), (1000, 0, 300, 1200));
        // Handed straight to a depot isn't sold to the carrier.
        assert_eq!(p.targets[1].provided, 0);
        assert!(!p.met);
        assert_eq!(p.provided, 1000);
        assert_eq!(p.required, 1500);

        // Gold isn't a target, so it doesn't count for u2.
        assert_eq!(p.contributors[0], Contributor { user_id: 1, username: "u1".into(), to_carrier: 600, to_depot: 100 });
        assert_eq!(p.contributors[1].to_carrier, 600);
    }

    #[test]
    fn depot_targets_use_the_depot_count() {
        let p = compute(
            vec![target("steel", 1000, Some(1000)), target("cmmcomposite", 400, Some(250))],
            &HashMap::new(),
            &[delivered(1, "depot", "steel", 100)],
        );
        assert_eq!(p.targets[0].remaining, 0);
        assert_eq!(p.targets[1].remaining, 150);
        assert!(!p.met);

        let done = compute(vec![target("steel", 10, Some(10))], &HashMap::new(), &[]);
        assert!(done.met);
        assert_eq!(done.fraction, 1.0);
    }

    #[test]
    fn parses_journal_lines() {
        use crate::journal_definitions::{HaulingEvent, commodity_key};

        let depot: HaulingEvent = serde_json::from_str(r#"{ "timestamp":"2026-10-27T12:00:00Z", "event":"ColonisationConstructionDepot",
            "MarketID":3960950274, "ConstructionProgress":0.25, "ConstructionComplete":false, "ConstructionFailed":false,
            "ResourcesRequired":[ { "Name":"$CMMComposite_name;", "Name_Localised":"CMM Composite", "RequiredAmount":4319, "ProvidedAmount":100, "Payment":6788 } ],
            "_StarSystem":"Col 285 Sector AB-C d1", "_StationName":"Construction Site" }"#).unwrap();
        let HaulingEvent::ColonisationConstructionDepot(d) = depot else { panic!() };
        assert_eq!(commodity_key(&d.resources_required[0].name), "cmmcomposite");
        assert_eq!(d.station_name.as_deref(), Some("Construction Site"));

        let sell: HaulingEvent = serde_json::from_str(r#"{ "timestamp":"2026-10-27T12:00:00Z", "event":"MarketSell",
            "MarketID":3700000000, "Type":"steel", "Count":720, "SellPrice":5000, "TotalSale":3600000, "AvgPricePaid":4000 }"#).unwrap();
        assert!(matches!(sell, HaulingEvent::MarketSell(s) if s.count == 720 && commodity_key(&s.commodity) == "steel"));

        let given: HaulingEvent = serde_json::from_str(r#"{ "timestamp":"2026-10-27T12:00:00Z", "event":"ColonisationContribution",
            "MarketID":3960950274, "Contributions":[ { "Name":"$Steel_name;", "Name_Localised":"Steel", "Amount":23 } ] }"#).unwrap();
        assert!(matches!(given, HaulingEvent::ColonisationContribution(c) if c.contributions[0].amount == 23));
    }

    #[test]
    fn nothing_to_do_is_not_done() {
        let p = compute(Vec::new(), &HashMap::new(), &[]);
        assert!(!p.met);
        assert_eq!(p.fraction, 0.0);
    }
}
