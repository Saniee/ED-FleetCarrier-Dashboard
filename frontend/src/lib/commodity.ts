import type { Commodity } from './types/market';
import { formatCredits, formatNumber } from './format';

/**
 * Journal symbols wrap the display name, e.g. `$iridium_name;` or
 * `$MARKET_category_metals;`. `Name_Localised` / `Category_Localised` carry the
 * real label; this is the fallback for the few entries that omit them.
 */
function unwrapSymbol(symbol: string): string {
	return symbol
		.replace(/^\$/, '')
		.replace(/;$/, '')
		.replace(/_name$/, '')
		.replace(/^MARKET_category_/, '')
		.replace(/_/g, ' ');
}

/** Display name for a commodity. */
export function commodityName(c: Commodity): string {
	return c.name_localised ?? unwrapSymbol(c.name);
}

/** Display name for a commodity's category, or an em dash when unknown. */
export function commodityCategory(c: Commodity): string {
	if (c.category_localised) return c.category_localised;
	return c.category ? unwrapSymbol(c.category) : '—';
}

/**
 * A price or a quantity, or an em dash when there is nothing to show.
 *
 * Zero is not a value in a `Market` payload. A price of 0 means that side of the
 * market is not traded at all, and a stock or demand of 0 means there is nothing
 * behind it. Rendering the zero would read as "free" and as a real quantity, so
 * both collapse to a dash.
 */
function orDash(value: number | null, format: (value: number) => string): string {
	return value ? format(value) : '—';
}

/** A price in credits, whichever side of the market it belongs to. */
export function commodityPrice(value: number | null): string {
	return orDash(value, formatCredits);
}

/** How much of the commodity the carrier has, or still wants. */
export function commodityQuantity(value: number | null): string {
	return orDash(value, formatNumber);
}

/**
 * Whether a commodity is actually being traded.
 *
 * A carrier's orders are open-ended, and `Market.json` keeps reporting the price
 * after the quantity behind it runs out: a sale sitting at `Stock: 0` is still
 * listed in the file but is not selling anything, and the in-game market does not
 * show it. A purchase at `Demand: 0` is the same. A side only counts when it has
 * both a price and something behind it — `BuyPrice`/`Stock` for a sale,
 * `SellPrice`/`Demand` for a purchase.
 */
export function isTraded(c: Commodity): boolean {
	const sells = (c.buy_price ?? 0) > 0 && (c.stock ?? 0) > 0;
	const buys = (c.sell_price ?? 0) > 0 && (c.demand ?? 0) > 0;
	return sells || buys;
}
