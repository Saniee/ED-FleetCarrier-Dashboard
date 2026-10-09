/**
 * Formatting for journal/DB values. The locale is pinned so output is identical
 * on every machine (numbers `1.234.567,89`, dates `19.09.2026, 22:57:29`);
 * change LOCALE to switch, e.g. 'fr-FR'.
 */
export const LOCALE = 'de-DE';

const EMPTY = '—';

export interface FormatTimestampOptions {
	utc?: boolean;
	showZone?: boolean;
}

/**
 * Format a journal (`2026-09-19T22:57:29Z`) or `updated_at` (full offset)
 * timestamp. Without a timezone it is treated as UTC; shown in the viewer's
 * timezone unless `{ utc: true }`.
 */
export function formatTimestamp(
	value: string,
	{ utc = false, showZone = false }: FormatTimestampOptions = {},
): string {
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

/** Format a duration as a countdown (`14:32`, `1:02:09`), clamped at zero. */
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

/** Parse a journal/DB timestamp to epoch ms, assuming UTC without a timezone; `null` if unparseable. */
export function parseTimestamp(value: string): number | null {
	const hasZone = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(value);
	const ms = new Date(hasZone ? value : `${value}Z`).getTime();
	return Number.isNaN(ms) ? null : ms;
}
