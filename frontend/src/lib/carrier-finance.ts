/**
 * Upkeep runway. Upkeep is taken in the weekly maintenance, Thursday 07:00 UTC
 * (the backend's `finances::week_start` uses the same tick). The journal never
 * reports what upkeep costs, so the figure is the owner's, copied from the
 * in-game finance screen.
 */

const UPKEEP_DAY = 4; // Thursday, as Date#getUTCDay counts
const UPKEEP_HOUR = 7;
const WEEK = 7 * 24 * 3600 * 1000;

/** The next upkeep tick after `now` (epoch ms). */
export function nextUpkeep(now: number): number {
	const d = new Date(now);
	const tick = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), UPKEEP_HOUR);
	const ahead = (UPKEEP_DAY - d.getUTCDay() + 7) % 7;
	const candidate = tick + ahead * 24 * 3600 * 1000;
	return candidate > now ? candidate : candidate + WEEK;
}

export interface Runway {
	/** Upkeep payments the balance covers. */
	weeks: number;
	/** The first tick the balance can't pay, epoch ms. */
	shortAt: number;
}

/** `null` without both a balance and an upkeep figure. */
export function runway(balance: number | null, upkeep: number | null, now: number): Runway | null {
	if (balance === null || !upkeep || upkeep <= 0) return null;
	const weeks = Math.max(0, Math.floor(balance / upkeep));
	return { weeks, shortAt: nextUpkeep(now) + weeks * WEEK };
}
