import type { Carrier, CarrierEventRow } from './types/carrier';

/** Connection state as shown in the status badge. */
export type CarrierStatus = 'connecting' | 'connected' | 'OFFLINE';

/** Exponential backoff for stream rebuilds. */
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

/** Rows requested for the history table. */
export const HISTORY_LIMIT = 20;

/**
 * Live carrier feed: the SSE stream, its reconnect backoff, a liveness probe,
 * and the history fetch that follows whichever carrier is current.
 *
 * Call `start()` on mount and `stop()` on teardown.
 */
export function createCarrierFeed(initial: Carrier | null) {
	let carrier = $state<Carrier | null>(initial);
	let events = $state<CarrierEventRow[] | null>(null);
	let status = $state<CarrierStatus>('connecting');

	let source: EventSource | undefined;
	let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	let pingTimer: ReturnType<typeof setInterval> | undefined;
	let attempt = 0;

	// Guards against a slow history response landing after a newer one.
	let historySeq = 0;

	async function loadHistory(carrierId: number) {
		const seq = ++historySeq;
		try {
			const res = await fetch(`/api/carrier/${carrierId}/events?limit=${HISTORY_LIMIT}`);
			if (!res.ok) return;
			const rows: CarrierEventRow[] = await res.json();
			if (seq === historySeq) events = rows;
		} catch {
			// Offline; the next update retries.
		}
	}

	function connect() {
		source = new EventSource('/api/carrier/stream');

		source.onopen = () => {
			attempt = 0;
			status = 'connected';
		};

		source.addEventListener('carrier', (e) => {
			const next = JSON.parse((e as MessageEvent).data) as Carrier;
			carrier = next;
			void loadHistory(next.carrier_id);
		});

		source.onerror = () => {
			// The browser retries on its own while CONNECTING. A CLOSED stream is
			// the case it gives up on — an error response, e.g. the proxy's 500
			// when the backend is down, since that isn't text/event-stream.
			if (source?.readyState !== EventSource.CLOSED) return;

			source.close();
			source = undefined;
			status = 'OFFLINE';
			scheduleReconnect();
		};
	}

	function scheduleReconnect() {
		if (reconnectTimer !== undefined) return;

		// Exponential backoff, capped. attempt resets on a successful open.
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
				// Drop the dead stream so it cannot linger; the probe reopens it.
				source?.close();
				source = undefined;
				if (reconnectTimer !== undefined) {
					clearTimeout(reconnectTimer);
					reconnectTimer = undefined;
				}
			}
			return;
		}

		// API is up again: rebuild the stream immediately.
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

	function start() {
		// Seed history from the load function's carrier before the first event.
		if (carrier) void loadHistory(carrier.carrier_id);

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
		get status() {
			return status;
		},
		start,
		stop,
	};
}
