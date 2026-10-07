/**
 * Types for the carrier market API.
 *
 * The API answers with an envelope of two parts, mirroring the two tables the
 * backend stores:
 *
 *   - `event`       — the header of the last `Market` journal event
 *                     (`carrier_markets`): which station, where, and when;
 *   - `commodities` — every item that event listed (`market_commodities`).
 *
 * As with the carrier types, rows are serialized with Postgres `to_jsonb`, so
 * keys are the snake_case column names and nullable columns arrive as `null`
 * rather than being omitted.
 */

export interface MarketHeader {
	carrier_id: number;
	/** The raw `MarketID`; equal to `carrier_id` for a fleet carrier. */
	market_id: number;
	station_name: string | null;
	station_type: string | null;
	star_system: string | null;
	carrier_docking_access: string | null;
	/** The event's own timestamp, e.g. `"2026-10-04T13:56:55Z"`. */
	timestamp: string | null;
	updated_at: string;
}

export interface Commodity {
	carrier_id: number;
	/** The journal's commodity id; unique within a market. */
	commodity_id: number;
	/** Journal symbol, e.g. `$iridium_name;`. Always present. */
	name: string;
	name_localised: string | null;
	category: string | null;
	category_localised: string | null;
	/** What the carrier pays; 0 when it has no buy order for this commodity. */
	buy_price: number | null;
	/** What the carrier charges; 0 when it has no sell order. */
	sell_price: number | null;
	mean_price: number | null;
	/** 0-3, 3 being the highest. */
	stock_bracket: number | null;
	demand_bracket: number | null;
	stock: number | null;
	demand: number | null;
	consumer: boolean | null;
	producer: boolean | null;
	rare: boolean | null;
}

export interface MarketSnapshot {
	event: MarketHeader;
	commodities: Commodity[];
}
