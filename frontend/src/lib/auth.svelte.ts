/**
 * Login session for the dashboard: the token from `POST /api/auth/login`, sent
 * as `Authorization: Bearer` and kept in localStorage until it expires (7 days).
 */

export interface User {
	id: number;
	username: string;
	role: 'user' | 'admin';
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
	if (res.status === 401) return 'Wrong username or password.';
	if (res.status === 403) return 'This account has been disabled.';
	if (!res.ok) return failure(res);
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

/**
 * Re-read the user from the server: the stored copy can be stale (a role
 * changed, the account disabled, a session from before roles existed). A 401
 * ends the session; a network failure keeps it.
 */
async function refresh() {
	if (!session) return;
	const res = await api('/api/auth/me').catch(() => null);
	if (!res) return;
	if (res.status === 401) {
		store(null);
		return;
	}
	if (res.ok && session) store({ ...session, user: (await res.json()) as User });
}

export const auth = {
	get user(): User | null {
		return session?.user ?? null;
	},
	/** The session token, for requests that cannot go through `api()` (the event stream). */
	get token(): string | null {
		return session?.token ?? null;
	},
	get isAdmin(): boolean {
		return session?.user.role === 'admin';
	},
	login,
	register,
	logout,
	refresh
};
