/**
 * Fleet carrier fuel maths.
 *
 * Tritium per jump, per the community-verified formula:
 *
 *   fuelUsage = ceil(5 + (distance × (capacityUsed + fuelInReservoir + carrierMass)) / 200000)
 *
 * Sanity checks that pin the constants:
 *   - empty carrier, 500 ly: ceil(5 + 500 × 25000 / 200000) = 68 t  (documented minimum)
 *   - fully loaded, 500 ly:  ceil(5 + 500 × 51000 / 200000) = 133 t (documented maximum)
 */

/**
 * Hull mass in tons, by carrier type. Squadron Carriers are lighter, so they
 * burn less fuel for the same distance and load.
 */
export const FLEET_CARRIER_MASS = 25000;
export const SQUADRON_CARRIER_MASS = 15000;

/** Fuel divisor from the formula. */
const FUEL_DIVISOR = 200000;

/** Flat fuel cost added to every jump. */
const BASE_FUEL = 5;

/** A carrier can never jump further than this, in light years. */
export const MAX_JUMP_RANGE = 500;

/**
 * Tritium burned by one jump of `distance` light years.
 *
 * Defaults to a Fleet Carrier's mass. The journal does carry a `CarrierType`
 * field, but it is not stored yet, so a Squadron Carrier would need that value
 * passed in explicitly.
 */
export function fuelPerJump(
	distance: number,
	capacityUsed: number,
	fuelInReservoir: number,
	carrierMass = FLEET_CARRIER_MASS,
): number {
	const mass = Math.max(0, capacityUsed) + Math.max(0, fuelInReservoir) + carrierMass;
	return Math.ceil(BASE_FUEL + (distance * mass) / FUEL_DIVISOR);
}

/**
 * How many jumps the current fuel allows at a given cost per jump.
 * Returns 0 rather than Infinity when the cost is degenerate.
 */
export function jumpsLeft(fuel: number, perJump: number): number {
	if (perJump <= 0) return 0;
	return Math.max(0, Math.floor(fuel / perJump));
}
