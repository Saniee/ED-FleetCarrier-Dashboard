export interface SseHandlers {
	onopen?: () => void;
	/** The stream ended or failed to open. Not called after `close()`. */
	onclose: () => void;
	onmessage: (event: string, data: string) => void;
}

/**
 * Server-sent events over `fetch`, so the session token goes in the
 * `Authorization` header instead of a URL. No automatic reconnect: the caller
 * owns the backoff.
 */
export function openStream(url: string, token: string | null, handlers: SseHandlers) {
	const controller = new AbortController();

	void (async () => {
		try {
			const res = await fetch(url, {
				headers: {
					Accept: 'text/event-stream',
					...(token ? { Authorization: `Bearer ${token}` } : {})
				},
				cache: 'no-store',
				signal: controller.signal
			});
			if (!res.ok || !res.body) throw new Error(`stream ${res.status}`);
			handlers.onopen?.();

			const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
			let buffer = '';
			let event = 'message';
			let data: string[] = [];

			for (;;) {
				const { done, value } = await reader.read();
				if (done) break;
				buffer += value;

				let nl: number;
				while ((nl = buffer.search(/\r\n|\r|\n/)) !== -1) {
					const line = buffer.slice(0, nl);
					buffer = buffer.slice(buffer[nl] === '\r' && buffer[nl + 1] === '\n' ? nl + 2 : nl + 1);

					if (line === '') {
						if (data.length) handlers.onmessage(event, data.join('\n'));
						event = 'message';
						data = [];
					} else if (line.startsWith('event:')) {
						event = line.slice(6).trimStart();
					} else if (line.startsWith('data:')) {
						data.push(line.slice(5).replace(/^ /, ''));
					}
				}
			}
		} catch {
		}
		if (!controller.signal.aborted) handlers.onclose();
	})();

	return { close: () => controller.abort() };
}
