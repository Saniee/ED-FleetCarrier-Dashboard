/**
 * Types for the carrier market API: an envelope of `event` (the last `Market`
 * header, `carrier_markets`) and `commodities` (its items, `market_commodities`).
 * Rows follow the same `to_jsonb` rules as the carrier types.
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
