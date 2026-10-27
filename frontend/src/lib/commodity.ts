import type { Commodity } from './types/market';
import { formatCredits, formatNumber } from './format';

/**
 * Journal symbols wrap the display name (`$iridium_name;`); this is the fallback
 * for entries missing `Name_Localised` / `Category_Localised`.
 */
function unwrapSymbol(symbol: string): string {
	return symbol
		.replace(/^\$/, '')
		.replace(/;$/, '')
		.replace(/_name$/, '')
		.replace(/^MARKET_category_/, '')
		.replace(/_/g, ' ');
}

export function commodityName(c: Commodity): string {
	return c.name_localised ?? unwrapSymbol(c.name);
}

export function commodityCategory(c: Commodity): string {
	if (c.category_localised) return c.category_localised;
	return c.category ? unwrapSymbol(c.category) : '—';
}

/**
 * A price or quantity, or an em dash for zero: 0 means "not traded" or "nothing
 * behind it", and rendering it would read as free or as real stock.
 */
function orDash(value: number | null, format: (value: number) => string): string {
	return value ? format(value) : '—';
}

export function commodityPrice(value: number | null): string {
	return orDash(value, formatCredits);
}

export function commodityQuantity(value: number | null): string {
	return orDash(value, formatNumber);
}

/**
 * Whether a commodity is actually being traded. `Market.json` keeps reporting a
 * price after its quantity runs out, so a side counts only with both a price and
 * stock/demand (`BuyPrice`/`Stock` to sell, `SellPrice`/`Demand` to buy).
 */
export function isTraded(c: Commodity): boolean {
	const sells = (c.buy_price ?? 0) > 0 && (c.stock ?? 0) > 0;
	const buys = (c.sell_price ?? 0) > 0 && (c.demand ?? 0) > 0;
	return sells || buys;
}
