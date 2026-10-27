/** `GET /api/carriers/{callsign}/finances`, see `backend/src/finances.rs`. */

export interface FinanceWeek {
	/** The upkeep tick that opens the week. */
	start: string;
	/** Balance change less the owner's transfers; `null` with nothing reported. */
	earned: number | null;
	transferred: number;
	closing: number | null;
	/** `earned` reaches back into weeks with no report of their own. */
	spans_weeks: boolean;
}

export interface Transfer {
	at: string;
	/** Positive for a deposit, negative for a withdrawal. */
	amount: number;
	balance: number | null;
}

export interface FinanceHistory {
	weeks: FinanceWeek[];
	next_upkeep: string;
	transfers: Transfer[];
}
