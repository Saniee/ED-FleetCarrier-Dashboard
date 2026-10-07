/**
 * Login session for the dashboard.
 *
 * The session token comes from `POST /api/auth/login` and is sent as
 * `Authorization: Bearer`. It is kept in localStorage so a reload stays logged
 * in; the backend expires it (7 days) and `load()` drops one that has lapsed.
 */

export interface User {
	id: number;
	username: string;
}

interface Session {
	token: string;
	expires_at: string;
	user: User;
}

const KEY = 'session';

function load(): Session | null {
	try {
		const stored = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Session | null;
		if (stored && new Date(stored.expires_at) > new Date()) return stored;
	} catch {
		// Unavailable or corrupt storage: treat as logged out.
	}
	return null;
}

let session = $state<Session | null>(load());

function store(next: Session | null) {
	session = next;
	try {
		if (next) localStorage.setItem(KEY, JSON.stringify(next));
		else localStorage.removeItem(KEY);
	} catch {
		// Not persisted; the session still works until reload.
	}
}

export function api(path: string, init: RequestInit = {}): Promise<Response> {
	const headers = new Headers(init.headers);
	if (session) headers.set('Authorization', `Bearer ${session.token}`);
	if (init.body !== undefined && !headers.has('Content-Type')) {
		headers.set('Content-Type', 'application/json');
	}
	return fetch(path, { ...init, headers });
}

async function failure(res: Response): Promise<string> {
	const text = await res.text().catch(() => '');
	return text || `Request failed (${res.status})`;
}

/** Log in. Resolves to an error message, or '' on success. */
async function login(username: string, password: string): Promise<string> {
	const res = await api('/api/auth/login', {
		method: 'POST',
		body: JSON.stringify({ username, password })
	});
	if (!res.ok) return res.status === 401 ? 'Wrong username or password.' : failure(res);
	store((await res.json()) as Session);
	return '';
}

/** Create the account, then log in with it. Resolves to an error message, or ''. */
async function register(username: string, password: string): Promise<string> {
	const res = await api('/api/auth/register', {
		method: 'POST',
		body: JSON.stringify({ username, password })
	});
	if (!res.ok) return res.status === 409 ? 'Username already taken.' : failure(res);
	return login(username, password);
}

async function logout() {
	await api('/api/auth/logout', { method: 'POST' }).catch(() => {});
	store(null);
}

export const auth = {
	get user(): User | null {
		return session?.user ?? null;
	},
	/** The session token, for requests that cannot go through `api()` (EventSource). */
	get token(): string | null {
		return session?.token ?? null;
	},
	login,
	register,
	logout
};
