import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// Keep all API calls origin-relative ("/api/...") so the same code works
		// in dev and behind Caddy in prod with no CORS. Covers both the ingest
		// POST and the SSE stream.
		proxy: {
			'/api': {
				target: 'http://localhost:8080',
				changeOrigin: true
			}
		}
	}
});
