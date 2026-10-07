import type { Visibility } from '../visibility';

/**
 * Types for the carrier API.
 *
 * The API serializes each row with Postgres `to_jsonb(carriers)`, which shapes
 * the JSON in ways worth knowing:
 *
 *  - keys are the snake_case column names, exactly as in the DB;
 *  - nullable columns are present as `null`, they are NOT omitted — so these
 *    are `| null`, not optional `?`;
 *  - `bigint` columns arrive as JSON numbers (safe here: Elite IDs and system
 *    addresses stay well under `Number.MAX_SAFE_INTEGER`);
 *  - `updated_at` is an ISO-8601 string with an offset, e.g.
 *    `"2026-09-28T16:51:08.077942+00:00"`;
 *  - `text[]` becomes a JSON array of strings;
 *  - jsonb collections keep the journal's PascalCase keys, because they are
 *    stored from the parsed event structs rather than re-mapped.
 */

/** One entry of `crew`. Only activated roles carry `Enabled` / `CrewName`. */
export interface CrewMember {
	CrewRole: string;
	Activated: boolean;
	Enabled: boolean | null;
	CrewName: string | null;
}

export interface Pack {
	PackTheme: string;
	PackTier: number;
}

/**
 * One entry of `trade_orders`, keyed by commodity (e.g. `"iridium"`).
 * A purchase order sets `PurchaseOrder`, a sale order sets `SaleOrder`.
 */
export interface TradeOrder {
	Price?: number;
	BlackMarket: boolean;
	PurchaseOrder?: number;
	SaleOrder?: number;
}

export interface StationEconomy {
	Name: string;
	Name_Localised: string | null;
	Proportion: number;
}

/**
 * A row of the `carriers` table — one fleet carrier's latest known state.
 * Every column except `carrier_id` and `updated_at` is nullable, because a
 * carrier's row is filled in gradually as different journal events arrive.
 */
export interface Carrier {
	// Identity
	carrier_id: number;
	callsign: string | null;
	name: string | null;
	market_id: number | null;

	// Purchase
	variant: string | null;
	purchase_price: number | null;
	purchased_at: string | null;
	bought_at_market: number | null;
	purchase_location: string | null;
	purchase_system_address: number | null;

	// Access
	docking_access: string | null;
	allow_notorious: boolean | null;

	// Fuel and range
	fuel_level: number | null;
	jump_range_curr: number | null;
	jump_range_max: number | null;

	// Space usage
	space_total_capacity: number | null;
	space_crew: number | null;
	space_cargo: number | null;
	space_cargo_reserved: number | null;
	space_ship_packs: number | null;
	space_module_packs: number | null;
	space_free: number | null;

	// Finance
	carrier_balance: number | null;
	reserve_balance: number | null;
	available_balance: number | null;
	reserve_percent: number | null;

	// Per-service tax rates
	tax_rate_shipyard: number | null;
	tax_rate_rearm: number | null;
	tax_rate_outfitting: number | null;
	tax_rate_refuel: number | null;
	tax_rate_repair: number | null;

	// Current location
	docked: boolean | null;
	star_system: string | null;
	system_address: number | null;
	star_x: number | null;
	star_y: number | null;
	star_z: number | null;
	body: string | null;
	body_id: number | null;
	body_type: string | null;
	population: number | null;

	// Current system
	system_allegiance: string | null;
	system_faction: string | null;
	system_economy: string | null;
	system_economy_localised: string | null;
	system_second_economy: string | null;
	system_second_economy_localised: string | null;
	system_government: string | null;
	system_government_localised: string | null;
	system_security: string | null;
	system_security_localised: string | null;

	// The carrier's own station info
	station_name: string | null;
	station_type: string | null;
	station_faction: string | null;
	station_government: string | null;
	station_government_localised: string | null;
	station_economy: string | null;
	station_economy_localised: string | null;
	station_services: string[] | null;
	station_economies: StationEconomy[] | null;

	// Scheduled jump (all cleared on CarrierJumpCancelled)
	jump_destination_system: string | null;
	jump_destination_system_address: number | null;
	jump_destination_body: string | null;
	jump_destination_body_id: number | null;
	jump_departure_time: string | null;

	// Nested collections
	crew: CrewMember[] | null;
	ship_packs: Pack[] | null;
	module_packs: Pack[] | null;
	trade_orders: Record<string, TradeOrder> | null;

	// Bookkeeping
	last_event_id: number | null;

	// Tenancy
	owner_id: number | null;
	visibility: Visibility;
	/** Location last reported by someone other than the owner; show a `*`. */
	location_unverified: boolean;
	updated_at: string;
}

/**
 * A row of `carrier_events` as returned by
 * `GET /api/carriers/{callsign}/events?limit=N`.
 *
 * `carrier` is null for events that could not be attributed to a carrier
 * (an on-foot `CarrierJump` whose system matched nothing).
 */
export interface CarrierEventRow {
	id: number;
	timestamp: string;
	event_name: string;
	json_data: CarrierEventJson;
	carrier: number | null;
}

export interface Faction {
	Name: string;
}

export interface SpaceUsage {
	TotalCapacity: number;
	Crew: number;
	Cargo: number;
	CargoSpaceReserved: number;
	ShipPacks: number;
	ModulePacks: number;
	FreeSpace: number;
}

/**
 * `CarrierStats.Finance`. The early report after purchase carries only the
 * three balances, so the rest are nullable.
 */
export interface Finance {
	CarrierBalance: number;
	ReserveBalance: number;
	AvailableBalance: number;
	ReservePercent: number | null;
	TaxRate_shipyard: number | null;
	TaxRate_rearm: number | null;
	TaxRate_outfitting: number | null;
	TaxRate_refuel: number | null;
	TaxRate_repair: number | null;
}

// Journal events, as stored in `carrier_events.json_data`
//
// These mirror the backend's `CarrierEvent` enum: an internally tagged union on
// `event`. Two consequences of that round-trip:
//   - optional fields are present as explicit `null`, never omitted;
//   - unknown journal keys (CarrierType, and CarrierNameChange's malformed `""`)
//     are dropped by the parser and do not appear here.

export interface CarrierJumpEvent {
	event: 'CarrierJump';
	timestamp: string;
	Docked: boolean;
	/** Only present on the on-foot variant. */
	OnFoot: boolean | null;
	// The whole station block is absent when the jump happened while on foot.
	StationName: string | null;
	StationType: string | null;
	/** Equals `CarrierID` for fleet carriers. */
	MarketID: number | null;
	StationFaction: Faction | null;
	StationGovernment: string | null;
	StationGovernment_Localised: string | null;
	StationServices: string[] | null;
	StationEconomy: string | null;
	StationEconomy_Localised: string | null;
	StationEconomies: StationEconomy[] | null;
	StarSystem: string;
	SystemAddress: number;
	StarPos: number[];
	SystemAllegiance: string;
	SystemEconomy: string;
	SystemEconomy_Localised: string | null;
	SystemSecondEconomy: string;
	SystemSecondEconomy_Localised: string | null;
	SystemGovernment: string;
	SystemGovernment_Localised: string | null;
	SystemSecurity: string;
	SystemSecurity_Localised: string | null;
	Population: number;
	Body: string;
	BodyID: number;
	BodyType: string;
	SystemFaction: Faction | null;
}

export interface CarrierBuyEvent {
	event: 'CarrierBuy';
	timestamp: string;
	CarrierID: number;
	BoughtAtMarket: number;
	Location: string;
	SystemAddress: number;
	Price: number;
	Variant: string;
	Callsign: string;
}

export interface CarrierStatsEvent {
	event: 'CarrierStats';
	timestamp: string;
	CarrierID: number;
	Callsign: string;
	Name: string;
	DockingAccess: string;
	AllowNotorious: boolean;
	FuelLevel: number;
	JumpRangeCurr: number;
	JumpRangeMax: number;
	SpaceUsage: SpaceUsage;
	Finance: Finance;
	Crew: CrewMember[];
	ShipPacks: Pack[];
	ModulePacks: Pack[];
}

export interface CarrierJumpRequestEvent {
	event: 'CarrierJumpRequest';
	timestamp: string;
	CarrierID: number;
	SystemName: string;
	/** Absent when the destination has no body. */
	Body: string | null;
	SystemAddress: number;
	BodyID: number;
	DepartureTime: string;
}

export interface CarrierJumpCancelledEvent {
	event: 'CarrierJumpCancelled';
	timestamp: string;
	CarrierID: number;
}

export interface CarrierBankTransferEvent {
	event: 'CarrierBankTransfer';
	timestamp: string;
	CarrierID: number;
	/** Exactly one of `Deposit` / `Withdraw` is set. */
	Deposit: number | null;
	Withdraw: number | null;
	PlayerBalance: number;
	CarrierBalance: number;
}

export interface CarrierDepositFuelEvent {
	event: 'CarrierDepositFuel';
	timestamp: string;
	CarrierID: number;
	Amount: number;
	Total: number;
}

export interface CarrierCrewServicesEvent {
	event: 'CarrierCrewServices';
	timestamp: string;
	CarrierID: number;
	CrewRole: string;
	/** Activate | Deactivate | Pause | Resume | Replace */
	Operation: string;
	CrewName: string;
}

export interface CarrierFinanceEvent {
	event: 'CarrierFinance';
	timestamp: string;
	CarrierID: number;
	CarrierBalance: number;
	ReserveBalance: number;
	AvailableBalance: number;
	ReservePercent: number;
	TaxRate_shipyard: number | null;
	TaxRate_rearm: number | null;
	TaxRate_outfitting: number | null;
	TaxRate_refuel: number | null;
	TaxRate_repair: number | null;
}

/** Shared shape for the two pack events. */
export interface PackOrderFields {
	timestamp: string;
	CarrierID: number;
	/** BuyPack | SellPack | RestockPack */
	Operation: string;
	PackTheme: string;
	PackTier: number;
	/** Exactly one of `Cost` / `Refund` is set. */
	Cost: number | null;
	Refund: number | null;
}

export interface CarrierShipPackEvent extends PackOrderFields {
	event: 'CarrierShipPack';
}

export interface CarrierModulePackEvent extends PackOrderFields {
	event: 'CarrierModulePack';
}

export interface CarrierTradeOrderEvent {
	event: 'CarrierTradeOrder';
	timestamp: string;
	CarrierID: number;
	BlackMarket: boolean;
	Commodity: string;
	Commodity_Localised: string | null;
	/** One of purchase / sale / cancel is set. */
	PurchaseOrder: number | null;
	SaleOrder: number | null;
	CancelTrade: boolean | null;
	/** Absent on CancelTrade orders. */
	Price: number | null;
}

export interface CarrierDockingPermissionEvent {
	event: 'CarrierDockingPermission';
	timestamp: string;
	CarrierID: number;
	DockingAccess: string;
	AllowNotorious: boolean;
}

export interface CarrierNameChangeEvent {
	event: 'CarrierNameChange';
	timestamp: string;
	CarrierID: number;
	Callsign: string;
	Name: string;
}

/** A carrier reporting its own position; always carries `CarrierID`, never `StarPos`. */
export interface CarrierLocationEvent {
	event: 'CarrierLocation';
	timestamp: string;
	CarrierID: number;
	StarSystem: string;
	SystemAddress: number;
	BodyID: number;
}

export type CarrierEventJson =
	| CarrierJumpEvent
	| CarrierBuyEvent
	| CarrierStatsEvent
	| CarrierJumpRequestEvent
	| CarrierJumpCancelledEvent
	| CarrierBankTransferEvent
	| CarrierDepositFuelEvent
	| CarrierCrewServicesEvent
	| CarrierFinanceEvent
	| CarrierShipPackEvent
	| CarrierModulePackEvent
	| CarrierTradeOrderEvent
	| CarrierDockingPermissionEvent
	| CarrierNameChangeEvent
	| CarrierLocationEvent;

export type CarrierEventName = CarrierEventJson['event'];

/** One row of `GET /api/carriers` (and `/api/carriers/mine`, which adds `visibility`). */
export interface CarrierListItem {
	carrier_id: number;
	callsign: string | null;
	name: string | null;
	variant?: string | null;
	star_system: string | null;
	docked?: boolean | null;
	docking_access?: string | null;
	location_unverified?: boolean;
	visibility?: Visibility;
	updated_at: string;
}

export interface CarrierPage {
	items: CarrierListItem[];
	page: number;
	per_page: number;
	total: number;
	total_pages: number;
}
