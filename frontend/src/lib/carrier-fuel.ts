/**
 * Tritium per jump, per the community-verified formula:
 *
 *   fuelUsage = ceil(5 + (distance × (capacityUsed + fuelInReservoir + carrierMass)) / 200000)
 *
 * Pinned by the documented extremes at 500 ly: 68 t empty, 133 t fully loaded.
 */

/** Hull mass in tons by carrier type; squadron carriers are lighter. */
export const FLEET_CARRIER_MASS = 25000;
export const SQUADRON_CARRIER_MASS = 15000;

const FUEL_DIVISOR = 200000;

const BASE_FUEL = 5;

export const MAX_JUMP_RANGE = 500;

export function isSquadronHull(carrierType: string | null | undefined): boolean {
	return carrierType === 'SquadronCarrier';
}

/** Hull mass for a journal `CarrierType`; unknown types count as Fleet Carriers. */
export function carrierMassFor(carrierType: string | null | undefined): number {
	return isSquadronHull(carrierType) ? SQUADRON_CARRIER_MASS : FLEET_CARRIER_MASS;
}

/** Tritium burned by one jump of `distance` light years; mass defaults to a Fleet Carrier's. */
export function fuelPerJump(
	distance: number,
	capacityUsed: number,
	fuelInReservoir: number,
	carrierMass = FLEET_CARRIER_MASS,
): number {
	const mass = Math.max(0, capacityUsed) + Math.max(0, fuelInReservoir) + carrierMass;
	return Math.ceil(BASE_FUEL + (distance * mass) / FUEL_DIVISOR);
}

/** Returns 0 rather than Infinity when the cost per jump is degenerate. */
export function jumpsLeft(fuel: number, perJump: number): number {
	if (perJump <= 0) return 0;
	return Math.max(0, Math.floor(fuel / perJump));
}

export interface RouteFuel {
	jumps: number;
	/** Tritium for the whole route. */
	total: number;
	/** What the tank is short by, or 0 when it covers the route. */
	short: number;
}

/**
 * Tritium for `distance` light years in jumps of at most `range`. A straight
 * line split into equal jumps, so a plotted route runs a little longer. Each
 * jump burns the tank down, and a lighter tank costs less on the next jump.
 * Past an empty tank the owner refills from the hold; that tritium is already
 * in `capacityUsed`, which stays as it was, so the estimate errs high.
 */
export function routeFuel(
	distance: number,
	range: number,
	capacityUsed: number,
	fuel: number,
	carrierMass = FLEET_CARRIER_MASS,
): RouteFuel {
	if (distance <= 0 || range <= 0) return { jumps: 0, total: 0, short: 0 };

	const jumps = Math.ceil(distance / range);
	const hop = distance / jumps;
	let tank = Math.max(0, fuel);
	let total = 0;
	for (let i = 0; i < jumps; i++) {
		const cost = fuelPerJump(hop, capacityUsed, tank, carrierMass);
		total += cost;
		tank = Math.max(0, tank - cost);
	}
	return { jumps, total, short: Math.max(0, total - Math.max(0, fuel)) };
}
