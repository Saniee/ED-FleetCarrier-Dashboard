import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => ({
	plugins: [sveltekit()],
	server: {
		// Keep all API calls origin-relative ("/api/...") so the same code works
		// in dev and behind Caddy in prod with no CORS. Covers both the ingest
		// POST and the SSE stream.
		proxy: {
			'/api': {
				// 127.0.0.1, not localhost: the backend binds IPv4 only, and
				// Node resolves localhost to ::1 first, which stalls the connect.
				// API_TARGET points a second dev server at another backend.
				target: loadEnv(mode, '.', '').API_TARGET || 'http://127.0.0.1:8080',
				changeOrigin: true
			}
		}
	}
}));
