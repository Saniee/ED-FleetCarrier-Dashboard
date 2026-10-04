#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Internally tagged on the journal `event` field.
/// `parse_carrier_event` returns `None` for anything that isn't a known carrier
/// event, so it doubles as the filter for a journal tailer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "PascalCase")]
pub enum CarrierEvent {
    CarrierJump(CarrierJump),
    CarrierBuy(CarrierBuy),
    CarrierStats(CarrierStats),
    CarrierJumpRequest(CarrierJumpRequest),
    CarrierJumpCancelled(CarrierJumpCancelled),
    CarrierBankTransfer(CarrierBankTransfer),
    CarrierDepositFuel(CarrierDepositFuel),
    CarrierCrewServices(CarrierCrewServices),
    CarrierFinance(CarrierFinance),
    CarrierShipPack(PackOrder),
    CarrierModulePack(PackOrder),
    CarrierTradeOrder(CarrierTradeOrder),
    CarrierDockingPermission(CarrierDockingPermission),
    #[serde(rename = "CarrierNameChange")]
    CarrierNameChange(CarrierNameChange),
    CarrierLocation(CarrierLocation),
}

macro_rules! carrier_event_accessors {
    (
        with_id: [$($with_id:ident),+ $(,)?],
        jump: [$($jump:ident),* $(,)?] $(,)?
    ) => {
        impl CarrierEvent {
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$with_id(_) => stringify!($with_id),)+
                    $(Self::$jump(_) => stringify!($jump),)*
                }
            }

            pub fn timestamp(&self) -> &str {
                match self {
                    $(Self::$with_id(e) => &e.timestamp,)+
                    $(Self::$jump(e) => &e.timestamp,)*
                }
            }

            /// The carrier this event belongs to.
            ///
            /// `CarrierJump` carries no `CarrierID`, but for fleet carriers the
            /// station `MarketID` equals the `CarrierID`. The on-foot variant
            /// omits `MarketID` entirely and returns `None`.
            pub fn carrier_id(&self) -> Option<i64> {
                match self {
                    $(Self::$with_id(e) => Some(e.carrier_id),)+
                    $(Self::$jump(e) => e.market_id,)*
                }
            }
        }
    };
}


carrier_event_accessors! {
    with_id: [
        CarrierBuy,
        CarrierStats,
        CarrierJumpRequest,
        CarrierJumpCancelled,
        CarrierBankTransfer,
        CarrierDepositFuel,
        CarrierCrewServices,
        CarrierFinance,
        CarrierShipPack,
        CarrierModulePack,
        CarrierTradeOrder,
        CarrierDockingPermission,
        CarrierNameChange,
        CarrierLocation,
    ],
    jump: [
        CarrierJump,
    ],
}

/// Parse one raw journal line. `None` = not a known carrier event (or malformed).
pub fn parse_carrier_event(line: &str) -> Option<CarrierEvent> {
    serde_json::from_str(line).ok()
}

/// Parse one raw `Market` line, or a whole `Market.json` document.
///
/// `None` = malformed, or an event that is not `Market`. Markets are not part of
/// `CarrierEvent`: the event fires for every station the commander docks at, and
/// a market is its own domain rather than a carrier state change.
pub fn parse_market_event(line: &str) -> Option<MarketEvent> {
    let market: MarketEvent = serde_json::from_str(line).ok()?;
    (market.event == "Market").then_some(market)
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierJump {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    pub docked: bool,
    /// Present only on the on-foot variant of the event.
    pub on_foot: Option<bool>,
    // The station block is absent when the carrier jumps while the commander is
    // on foot in the destination system, so every one of these is optional.
    pub station_name: Option<String>,
    pub station_type: Option<String>,
    /// For fleet carriers this equals the carrier's `CarrierID`.
    #[serde(rename = "MarketID")]
    pub market_id: Option<i64>,
    pub station_faction: Option<Faction>,
    pub station_government: Option<String>,
    #[serde(rename = "StationGovernment_Localised")]
    pub station_government_localised: Option<String>,
    pub station_services: Option<Vec<String>>,
    pub station_economy: Option<String>,
    #[serde(rename = "StationEconomy_Localised")]
    pub station_economy_localised: Option<String>,
    pub station_economies: Option<Vec<StationEconomy>>,
    pub star_system: String,
    pub system_address: i64,
    pub star_pos: Vec<f64>,
    pub system_allegiance: String,
    pub system_economy: String,
    #[serde(rename = "SystemEconomy_Localised")]
    pub system_economy_localised: Option<String>,
    pub system_second_economy: String,
    #[serde(rename = "SystemSecondEconomy_Localised")]
    pub system_second_economy_localised: Option<String>,
    pub system_government: String,
    #[serde(rename = "SystemGovernment_Localised")]
    pub system_government_localised: Option<String>,
    pub system_security: String,
    #[serde(rename = "SystemSecurity_Localised")]
    pub system_security_localised: Option<String>,
    pub population: i64,
    pub body: String,
    #[serde(rename = "BodyID")]
    pub body_id: i64,
    pub body_type: String,
    /// Absent on some docked jumps.
    pub system_faction: Option<Faction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierBuy {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub bought_at_market: i64,
    pub location: String,
    pub system_address: i64,
    pub price: i64,
    pub variant: String,
    pub callsign: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierStats {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub callsign: String,
    pub name: String,
    pub docking_access: String,
    pub allow_notorious: bool,
    pub fuel_level: i64,
    pub jump_range_curr: f64,
    pub jump_range_max: f64,
    pub space_usage: SpaceUsage,
    pub finance: Finance,
    pub crew: Vec<Crew>,
    pub ship_packs: Vec<Pack>,
    pub module_packs: Vec<Pack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierJumpRequest {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub system_name: String,
    /// Absent when the destination has no body.
    pub body: Option<String>,
    pub system_address: i64,
    #[serde(rename = "BodyID")]
    pub body_id: i64,
    pub departure_time: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierJumpCancelled {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierBankTransfer {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    /// Exactly one of `deposit` / `withdraw` is present.
    pub deposit: Option<i64>,
    pub withdraw: Option<i64>,
    pub player_balance: i64,
    pub carrier_balance: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierDepositFuel {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub amount: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierCrewServices {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub crew_role: String,
    /// activate / deactivate / pause / resume / replace
    pub operation: String,
    pub crew_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierFinance {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub carrier_balance: i64,
    pub reserve_balance: i64,
    pub available_balance: i64,
    pub reserve_percent: i64,
    #[serde(rename = "TaxRate_shipyard")]
    pub tax_rate_shipyard: Option<i64>,
    #[serde(rename = "TaxRate_rearm")]
    pub tax_rate_rearm: Option<i64>,
    #[serde(rename = "TaxRate_outfitting")]
    pub tax_rate_outfitting: Option<i64>,
    #[serde(rename = "TaxRate_refuel")]
    pub tax_rate_refuel: Option<i64>,
    #[serde(rename = "TaxRate_repair")]
    pub tax_rate_repair: Option<i64>,
}

/// Shared shape for CarrierShipPack and CarrierModulePack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PackOrder {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    /// BuyPack / SellPack / RestockPack
    pub operation: String,
    pub pack_theme: String,
    pub pack_tier: i64,
    /// Exactly one of `cost` / `refund` is present.
    pub cost: Option<i64>,
    pub refund: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierTradeOrder {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub black_market: bool,
    pub commodity: String,
    #[serde(rename = "Commodity_Localised")]
    pub commodity_localised: Option<String>,
    /// One of purchase_order / sale_order / cancel_trade is set.
    pub purchase_order: Option<i64>,
    pub sale_order: Option<i64>,
    pub cancel_trade: Option<bool>,
    /// Absent on `CancelTrade` orders.
    pub price: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierDockingPermission {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub docking_access: String,
    pub allow_notorious: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
/// Note the event name: the journal emits `CarrierNameChange`, not
/// `CarrierNameChanged`. Frontier also emits a broken `"": "FleetCarrier"` key
/// on this event; serde ignores unknown keys so it causes no harm.
pub struct CarrierNameChange {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub callsign: String,
    pub name: String,
}

/// A carrier reporting its own position after a jump. Unlike `CarrierJump`
/// this always carries `CarrierID`, but never carries `StarPos`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CarrierLocation {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "CarrierID")]
    pub carrier_id: i64,
    pub star_system: String,
    pub system_address: i64,
    #[serde(rename = "BodyID")]
    pub body_id: i64,
}

// ---------------------------------------------------------------------------
// Nested types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SpaceUsage {
    pub total_capacity: i64,
    pub crew: i64,
    pub cargo: i64,
    pub cargo_space_reserved: i64,
    pub ship_packs: i64,
    pub module_packs: i64,
    pub free_space: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
/// The `Finance` block inside `CarrierStats`. Early reports carry only the
/// three balances; `ReservePercent` and the per-service tax rates appear once
/// they are configured.
pub struct Finance {
    pub carrier_balance: i64,
    pub reserve_balance: i64,
    pub available_balance: i64,
    pub reserve_percent: Option<i64>,
    #[serde(rename = "TaxRate_shipyard")]
    pub tax_rate_shipyard: Option<i64>,
    #[serde(rename = "TaxRate_rearm")]
    pub tax_rate_rearm: Option<i64>,
    #[serde(rename = "TaxRate_outfitting")]
    pub tax_rate_outfitting: Option<i64>,
    #[serde(rename = "TaxRate_refuel")]
    pub tax_rate_refuel: Option<i64>,
    #[serde(rename = "TaxRate_repair")]
    pub tax_rate_repair: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
/// Inactive crew roles report only `CrewRole` and `Activated`; `Enabled` and
/// `CrewName` appear once the service is activated.
pub struct Crew {
    pub crew_role: String,
    pub activated: bool,
    pub enabled: Option<bool>,
    pub crew_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Pack {
    pub pack_theme: String,
    pub pack_tier: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StationEconomy {
    pub name: String,
    #[serde(rename = "Name_Localised")]
    pub name_localised: Option<String>,
    pub proportion: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Faction {
    pub name: String,
}

// ---------------------------------------------------------------------------
// Market
// ---------------------------------------------------------------------------

/// The `StationType` a fleet carrier's market reports.
pub const FLEET_CARRIER_STATION_TYPE: &str = "FleetCarrier";

/// The `Market` journal event, as written to `Market.json`.
///
/// The journal line carries only the header; the commodity list lives in
/// `Market.json`, which repeats the header and adds `Items`. The ingest plugin
/// merges the two, so this is the whole payload either way — and it splits
/// cleanly into the event header and the commodities it lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketEvent {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "event")]
    pub event: String,
    /// For a fleet carrier this equals the carrier's `CarrierID`.
    #[serde(rename = "MarketID")]
    pub market_id: i64,
    pub station_name: Option<String>,
    pub station_type: Option<String>,
    pub carrier_docking_access: Option<String>,
    pub star_system: Option<String>,
    /// Absent from the journal line; present in `Market.json`.
    #[serde(default)]
    pub items: Vec<Commodity>,
}

impl MarketEvent {
    /// Whether this market belongs to a fleet carrier.
    ///
    /// The journal emits `Market` for every station; only the carrier's own
    /// market is tracked, so ingest drops anything else.
    pub fn is_fleet_carrier(&self) -> bool {
        self.station_type.as_deref() == Some(FLEET_CARRIER_STATION_TYPE)
    }
}

/// One entry of `MarketEvent::items`.
///
/// The numbers carry `serde(default)` on purpose: one unexpected item should not
/// reject a whole market, and a missing price already means "not traded" (0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Commodity {
    #[serde(rename = "id")]
    pub id: i64,
    /// Journal symbol, e.g. `$iridium_name;`.
    pub name: String,
    #[serde(rename = "Name_Localised")]
    pub name_localised: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "Category_Localised")]
    pub category_localised: Option<String>,
    /// What the carrier pays; 0 when it has no buy order for this commodity.
    #[serde(default)]
    pub buy_price: i64,
    /// What the carrier charges; 0 when it has no sell order.
    #[serde(default)]
    pub sell_price: i64,
    #[serde(default)]
    pub mean_price: i64,
    /// 0-3, 3 being the highest.
    #[serde(default)]
    pub stock_bracket: i64,
    #[serde(default)]
    pub demand_bracket: i64,
    #[serde(default)]
    pub stock: i64,
    #[serde(default)]
    pub demand: i64,
    #[serde(default)]
    pub consumer: bool,
    #[serde(default)]
    pub producer: bool,
    #[serde(default)]
    pub rare: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> CarrierEvent {
        parse_carrier_event(line).unwrap_or_else(|| panic!("failed to parse: {line}"))
    }

    // Real lines pulled from the live journal. These cover the shapes that the
    // original structs could not deserialize.
    const CARRIER_BUY: &str = r#"{ "timestamp":"2026-09-19T22:57:29Z", "event":"CarrierBuy", "CarrierType":"FleetCarrier", "CarrierID":3713341952, "BoughtAtMarket":3228864512, "Location":"L 190-21", "SystemAddress":2621834266987, "Price":5000000000, "Variant":"CarrierDockB", "Callsign":"KLJ-97Z" }"#;

    // First stats after purchase: Finance has only the three balances and every
    // crew entry is inactive with no Enabled/CrewName.
    const CARRIER_STATS: &str = r#"{ "timestamp":"2026-09-19T23:04:35Z", "event":"CarrierStats", "CarrierID":3713341952, "CarrierType":"FleetCarrier", "Callsign":"KLJ-97Z", "Name":"COMODO CLASS", "DockingAccess":"all", "AllowNotorious":false, "FuelLevel":500, "JumpRangeCurr":500.000000, "JumpRangeMax":500.000000, "PendingDecommission":false, "SpaceUsage":{ "TotalCapacity":25000, "Crew":0, "Cargo":0, "CargoSpaceReserved":0, "ShipPacks":0, "ModulePacks":0, "FreeSpace":25000 }, "Finance":{ "CarrierBalance":0, "ReserveBalance":0, "AvailableBalance":0 }, "Crew":[ { "CrewRole":"BlackMarket", "Activated":false }, { "CrewRole":"Captain", "Activated":true, "Enabled":true, "CrewName":"Norma Allison" } ], "ShipPacks":[ ], "ModulePacks":[ ] }"#;

    const CARRIER_FINANCE: &str = r#"{ "timestamp":"2026-09-19T23:19:46Z", "event":"CarrierFinance", "CarrierID":3713341952, "CarrierType":"FleetCarrier", "CarrierBalance":1000000000, "ReserveBalance":500000000, "AvailableBalance":500000000, "ReservePercent":50, "TaxRate_shipyard":0, "TaxRate_rearm":0, "TaxRate_outfitting":0, "TaxRate_refuel":5, "TaxRate_repair":0 }"#;

    const CARRIER_NAME_CHANGE: &str = r#"{ "timestamp":"2026-09-20T13:04:37Z", "event":"CarrierNameChange", "CarrierID":3713341952, "":"FleetCarrier", "Name":"Sly komodo", "Callsign":"KLJ-97Z" }"#;

    const CARRIER_LOCATION: &str = r#"{ "timestamp":"2026-09-19T22:57:29Z", "event":"CarrierLocation", "CarrierType":"FleetCarrier", "CarrierID":3713341952, "StarSystem":"L 190-21", "SystemAddress":2621834266987, "BodyID":12 }"#;

    // Docked jump: carries MarketID, which equals the CarrierID.
    const CARRIER_JUMP_DOCKED: &str = r#"{ "timestamp":"2026-09-20T14:58:12Z", "event":"CarrierJump", "Docked":true, "StationName":"KLJ-97Z", "StationType":"FleetCarrier", "MarketID":3713341952, "StationFaction":{ "Name":"FleetCarrier" }, "StationGovernment":"$government_Carrier;", "StationGovernment_Localised":"Private Ownership", "StationServices":[ "dock", "refuel" ], "StationEconomy":"$economy_Carrier;", "StationEconomy_Localised":"Private Enterprise", "StationEconomies":[ { "Name":"$economy_Carrier;", "Name_Localised":"Private Enterprise", "Proportion":1.000000 } ], "Taxi":false, "Multicrew":false, "StarSystem":"18 Camelopardalis", "SystemAddress":388434495835, "StarPos":[-57.93750,31.96875,-122.56250], "SystemAllegiance":"", "SystemEconomy":"$economy_None;", "SystemEconomy_Localised":"None", "SystemSecondEconomy":"$economy_None;", "SystemSecondEconomy_Localised":"None", "SystemGovernment":"$government_None;", "SystemGovernment_Localised":"None", "SystemSecurity":"$GAlAXY_MAP_INFO_state_anarchy;", "SystemSecurity_Localised":"Anarchy", "Population":0, "Body":"18 Camelopardalis A", "BodyID":2, "BodyType":"Star" }"#;

    // On-foot jump: no MarketID and no station block.
    const CARRIER_JUMP_ON_FOOT: &str = r#"{ "timestamp":"2026-09-20T00:10:01Z", "event":"CarrierJump", "Docked":false, "OnFoot":true, "StarSystem":"Pegasi Sector HX-T b3-0", "SystemAddress":667464508825, "StarPos":[-262.46875,-100.53125,-43.18750], "SystemAllegiance":"Independent", "SystemEconomy":"$economy_Industrial;", "SystemEconomy_Localised":"Industrial", "SystemSecondEconomy":"$economy_Extraction;", "SystemSecondEconomy_Localised":"Extraction", "SystemGovernment":"$government_Democracy;", "SystemGovernment_Localised":"Democracy", "SystemSecurity":"$SYSTEM_SECURITY_low;", "SystemSecurity_Localised":"Low Security", "Population":87593500, "Body":"Pegasi Sector HX-T b3-0 1 b", "BodyID":21, "BodyType":"Planet", "ControllingPower":"Pranav Antal", "Powers":[ "Pranav Antal" ], "PowerplayState":"Stronghold", "Factions":[ { "Name":"The Dark Wheel" } ], "SystemFaction":{ "Name":"The Dark Wheel" } }"#;

    #[test]
    fn parses_all_live_event_shapes() {
        for line in [
            CARRIER_BUY,
            CARRIER_STATS,
            CARRIER_FINANCE,
            CARRIER_NAME_CHANGE,
            CARRIER_LOCATION,
            CARRIER_JUMP_DOCKED,
            CARRIER_JUMP_ON_FOOT,
        ] {
            let _ = parse(line);
        }
    }

    #[test]
    fn every_carrier_event_has_a_timestamp_and_name() {
        for line in [CARRIER_BUY, CARRIER_STATS, CARRIER_LOCATION] {
            let event = parse(line);
            assert!(!event.name().is_empty());
            assert!(event.timestamp().ends_with('Z'));
        }
    }

    #[test]
    fn stats_accepts_bare_finance_and_inactive_crew() {
        match parse(CARRIER_STATS) {
            CarrierEvent::CarrierStats(stats) => {
                assert_eq!(stats.finance.reserve_percent, None);
                assert_eq!(stats.crew[0].enabled, None);
                assert_eq!(stats.crew[0].crew_name, None);
                assert_eq!(stats.crew[1].crew_name.as_deref(), Some("Norma Allison"));
            }
            other => panic!("expected CarrierStats, got {other:?}"),
        }
    }

    #[test]
    fn finance_uses_per_service_tax_rates() {
        match parse(CARRIER_FINANCE) {
            CarrierEvent::CarrierFinance(finance) => {
                assert_eq!(finance.tax_rate_refuel, Some(5));
                assert_eq!(finance.reserve_percent, 50);
            }
            other => panic!("expected CarrierFinance, got {other:?}"),
        }
    }

    #[test]
    fn name_change_event_uses_the_real_event_name() {
        match parse(CARRIER_NAME_CHANGE) {
            CarrierEvent::CarrierNameChange(change) => {
                assert_eq!(change.name, "Sly komodo");
            }
            other => panic!("expected CarrierNameChange, got {other:?}"),
        }
    }

    #[test]
    fn docked_jump_resolves_carrier_id_from_market_id() {
        let event = parse(CARRIER_JUMP_DOCKED);
        assert_eq!(event.carrier_id(), Some(3713341952));
    }

    #[test]
    fn on_foot_jump_has_no_carrier_id() {
        let event = parse(CARRIER_JUMP_ON_FOOT);
        assert_eq!(event.carrier_id(), None);
    }

    // A carrier market exactly as `Market.json` writes it: header plus items.
    const MARKET_CARRIER: &str = r#"{ "timestamp":"2026-10-04T13:56:55Z", "event":"Market", "MarketID":3713341952, "StationName":"KLJ-97Z", "StationType":"FleetCarrier", "CarrierDockingAccess":"all", "StarSystem":"Pegasi Sector HX-T b3-0", "Items":[ { "id":129046165, "Name":"$iridium_name;", "Name_Localised":"Iridium", "Category":"$MARKET_category_metals;", "Category_Localised":"Metals", "BuyPrice":799015, "SellPrice":0, "MeanPrice":0, "StockBracket":0, "DemandBracket":0, "Stock":0, "Demand":0, "Consumer":false, "Producer":false, "Rare":false } ] }"#;

    // The journal line for that same market: header only, no Items.
    const MARKET_JOURNAL_LINE: &str = r#"{ "timestamp":"2026-10-04T13:56:55Z", "event":"Market", "MarketID":3713341952, "StationName":"KLJ-97Z", "StationType":"FleetCarrier", "CarrierDockingAccess":"all", "StarSystem":"Pegasi Sector HX-T b3-0" }"#;

    // A regular station market, which ingest drops.
    const MARKET_STATION: &str = r#"{ "timestamp":"2017-10-05T10:10:34Z", "event":"Market", "MarketID":128678535, "StationName":"Black Hide", "StarSystem":"Wyrd", "Items":[ { "id":128049152, "Name":"$platinum_name;", "Name_Localised":"Platinum", "Category":"$MARKET_category_metals;", "Category_Localised":"Metals", "BuyPrice":0, "SellPrice":42220, "MeanPrice":19756, "StockBracket":0, "DemandBracket":3, "Stock":0, "Demand":9182, "Consumer":true, "Producer":false, "Rare":false } ] }"#;

    #[test]
    fn market_splits_into_event_and_commodities() {
        let market = parse_market_event(MARKET_CARRIER).expect("carrier market should parse");

        assert!(market.is_fleet_carrier());
        assert_eq!(market.market_id, 3713341952);
        assert_eq!(market.station_name.as_deref(), Some("KLJ-97Z"));
        assert_eq!(market.star_system.as_deref(), Some("Pegasi Sector HX-T b3-0"));
        assert_eq!(market.carrier_docking_access.as_deref(), Some("all"));

        assert_eq!(market.items.len(), 1);
        let iridium = &market.items[0];
        assert_eq!(iridium.id, 129046165);
        assert_eq!(iridium.name, "$iridium_name;");
        assert_eq!(iridium.name_localised.as_deref(), Some("Iridium"));
        assert_eq!(iridium.category_localised.as_deref(), Some("Metals"));
        assert_eq!(iridium.buy_price, 799015);
        assert_eq!(iridium.sell_price, 0);
        assert!(!iridium.consumer);
    }

    #[test]
    fn market_journal_line_parses_without_items() {
        let market = parse_market_event(MARKET_JOURNAL_LINE).expect("header-only line should parse");
        assert!(market.items.is_empty());
        assert!(market.is_fleet_carrier());
    }

    #[test]
    fn station_market_is_not_a_fleet_carrier() {
        let market = parse_market_event(MARKET_STATION).expect("station market should parse");
        assert!(!market.is_fleet_carrier());
    }

    #[test]
    fn non_market_events_are_rejected() {
        assert!(parse_market_event(CARRIER_STATS).is_none());
    }

    #[test]
    fn scans_live_journals() {
        let dir = match std::env::var("ED_JOURNAL_DIR") {
            Ok(d) => d,
            Err(_) => return,
        };
        let (mut total, mut failed) = (0usize, 0usize);
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.starts_with("Journal") {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap();
            for line in content.lines() {
                if !line.contains("\"event\":\"Carrier") {
                    continue;
                }
                total += 1;
                if parse_carrier_event(line).is_none() {
                    failed += 1;
                    eprintln!("FAILED: {}", &line[..line.len().min(200)]);
                }
            }
        }
        eprintln!("scanned {total} carrier lines, {failed} failed");
        assert_eq!(failed, 0);
    }
}