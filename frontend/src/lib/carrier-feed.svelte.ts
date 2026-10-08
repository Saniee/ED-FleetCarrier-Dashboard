import { api, auth } from './auth.svelte';
import { openStream } from './sse';
import type { Carrier, CarrierEventRow } from './types/carrier';
import type { MarketSnapshot } from './types/market';

export type CarrierStatus = 'connecting' | 'connected' | 'OFFLINE';

const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 15000;

/**
 * How often to probe the API for liveness.
 *
 * The stream cannot be trusted to report its own death: when the backend goes
 * away the dev proxy leaves the SSE connection open, so the browser never fires
 * `onerror` and the page would sit on stale data forever. The probe is what
 * actually notices the API disappearing — and coming back.
 */
const PING_MS = 5000;

export const HISTORY_LIMIT = 20;

/**
 * Live carrier feed: the SSE stream, its reconnect backoff, a liveness probe,
 * and the history fetch for one carrier, addressed by callsign.
 *
 * Call `start()` on mount and `stop()` on teardown.
 */
export function createCarrierFeed(initial: Carrier | null, callsign: string) {
	const base = `/api/carriers/${encodeURIComponent(callsign)}`;
	let carrier = $state<Carrier | null>(initial);
	let events = $state<CarrierEventRow[] | null>(null);
	let market = $state<MarketSnapshot | null>(null);
	let status = $state<CarrierStatus>('connecting');

	let source: { close: () => void } | undefined;
	let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	let pingTimer: ReturnType<typeof setInterval> | undefined;
	let attempt = 0;

	// Guards against a slow history response landing after a newer one.
	let historySeq = 0;

	async function loadHistory() {
		const seq = ++historySeq;
		try {
			const res = await api(`${base}/events?limit=${HISTORY_LIMIT}`);
			if (!res.ok) return;
			const rows: CarrierEventRow[] = await res.json();
			if (seq === historySeq) events = rows;
		} catch {
			// Offline; the next update retries.
		}
	}

	function connect() {
		source = openStream(`${base}/stream`, auth.token, {
			onopen: () => {
				attempt = 0;
				status = 'connected';
			},
			onmessage: (event, data) => {
				if (event === 'carrier') {
					carrier = JSON.parse(data) as Carrier;
					void loadHistory();
				} else if (event === 'market') {
					// The market arrives whole — header plus commodities — on the same
					// connection, both when a Market event is ingested and on subscribe,
					// so there is no separate fetch to keep in step.
					market = JSON.parse(data) as MarketSnapshot;
				}
			},
			onclose: () => {
				source = undefined;
				status = 'OFFLINE';
				scheduleReconnect();
			}
		});
	}

	function scheduleReconnect() {
		if (reconnectTimer !== undefined) return;

		const delay = Math.min(RECONNECT_BASE_MS * 2 ** attempt, RECONNECT_MAX_MS);
		attempt++;

		reconnectTimer = setTimeout(() => {
			reconnectTimer = undefined;
			status = 'connecting';
			connect();
		}, delay);
	}

	async function ping() {
		let alive = false;
		try {
			const res = await fetch('/api/healthz', { cache: 'no-store' });
			alive = res.ok;
		} catch {
			alive = false;
		}

		if (!alive) {
			if (status !== 'OFFLINE') {
				status = 'OFFLINE';
				source?.close();
				source = undefined;
				if (reconnectTimer !== undefined) {
					clearTimeout(reconnectTimer);
					reconnectTimer = undefined;
				}
			}
			return;
		}

		if (!source) {
			if (reconnectTimer !== undefined) {
				clearTimeout(reconnectTimer);
				reconnectTimer = undefined;
			}
			attempt = 0;
			status = 'connecting';
			connect();
		}
	}

	/** Re-read the carrier row, for changes the stream does not announce (claim, privacy). */
	async function reload() {
		try {
			const res = await api(base);
			if (res.ok) carrier = (await res.json()) as Carrier;
		} catch {
		}
	}

	function start() {
		if (carrier) void loadHistory();

		connect();
		pingTimer = setInterval(ping, PING_MS);
	}

	function stop() {
		if (reconnectTimer !== undefined) clearTimeout(reconnectTimer);
		if (pingTimer !== undefined) clearInterval(pingTimer);
		source?.close();
		source = undefined;
	}

	return {
		get carrier() {
			return carrier;
		},
		get events() {
			return events;
		},
		get market() {
			return market;
		},
		get status() {
			return status;
		},
		reload,
		start,
		stop,
	};
}
