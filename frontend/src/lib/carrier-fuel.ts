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
