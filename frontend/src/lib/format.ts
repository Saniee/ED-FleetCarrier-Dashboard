/**
 * Formatting for journal/DB values.
 *
 * The locale is pinned rather than taken from the viewer's system, so the
 * display is identical on every machine:
 *
 *   numbers : 1.234.567,89   (period thousands, comma decimal)
 *   dates   : 19.09.2026, 22:57:29   (day-first, 24-hour)
 *
 * Change LOCALE to switch convention. Some alternatives and what they produce:
 *   'de-DE' -> 1.234.567,89   19.09.2026
 *   'sk-SK' -> 1 234 567,89   19. 09. 2026
 *   'fr-FR' -> 1 234 567,89   19/09/2026
 */
export const LOCALE = 'de-DE';

const EMPTY = '—';

export interface FormatTimestampOptions {
	utc?: boolean;
	showZone?: boolean;
}

/**
 * Format a timestamp.
 *
 * Two shapes come out of the API:
 *   - journal timestamps, e.g. `"2026-09-19T22:57:29Z"` (UTC);
 *   - `updated_at`, e.g. `"2026-09-28T16:51:08.077942+00:00"` (full offset).
 *
 * Anything without explicit timezone information is treated as UTC (+0), which
 * is what the journal uses. Rendered in the viewer's system timezone by
 * default; pass `{ utc: true }` to pin it to UTC.
 */
export function formatTimestamp(
	value: string,
	{ utc = false, showZone = false }: FormatTimestampOptions = {},
): string {
	// No trailing `Z` and no ±hh:mm offset means "assume UTC".
	const hasZone = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(value);
	const date = new Date(hasZone ? value : `${value}Z`);

	// Unparseable: show the raw value rather than "Invalid Date".
	if (Number.isNaN(date.getTime())) return value;

	// Explicit components rather than dateStyle/timeStyle: those two cannot be
	// combined with `timeZoneName`, which V8 rejects with a TypeError.
	return new Intl.DateTimeFormat(LOCALE, {
		year: 'numeric',
		month: '2-digit',
		day: '2-digit',
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
		// Strict 24-hour clock: midnight is 00, never 24.
		hourCycle: 'h23',
		...(utc ? { timeZone: 'UTC' } : {}),
		...(showZone ? { timeZoneName: 'shortOffset' } : {}),
	}).format(date);
}

export function formatNumber(
	value: number | null | undefined,
	options?: Intl.NumberFormatOptions,
): string {
	if (value === null || value === undefined || Number.isNaN(value)) return EMPTY;
	return new Intl.NumberFormat(LOCALE, options).format(value);
}

export function formatCredits(value: number | null | undefined): string {
	return formatNumber(value, { maximumFractionDigits: 0 });
}

export function formatDecimal(
	value: number | null | undefined,
	digits = 2,
): string {
	return formatNumber(value, {
		minimumFractionDigits: digits,
		maximumFractionDigits: digits,
	});
}

/**
 * Format a duration as a countdown. Drops the hour segment below an hour:
 * `14:32`, `1:02:09`. Clamps at zero.
 */
export function formatCountdown(ms: number): string {
	const total = Math.max(0, Math.floor(ms / 1000));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const seconds = total % 60;
	const pad = (n: number) => String(n).padStart(2, '0');

	return hours > 0
		? `${hours}:${pad(minutes)}:${pad(seconds)}`
		: `${minutes}:${pad(seconds)}`;
}

/**
 * Parse a journal/DB timestamp to epoch milliseconds, assuming UTC (+0) when
 * the string carries no timezone. Returns `null` if unparseable.
 */
export function parseTimestamp(value: string): number | null {
	const hasZone = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(value);
	const ms = new Date(hasZone ? value : `${value}Z`).getTime();
	return Number.isNaN(ms) ? null : ms;
}
