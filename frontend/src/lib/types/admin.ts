import type { Visibility } from '$lib/visibility';

export type Role = 'user' | 'admin';

/** A row of `GET /api/admin/users`. */
export interface AdminUserRow {
	id: number;
	username: string;
	role: Role;
	created_at: string;
	disabled_at: string | null;
	disabled_reason: string | null;
	carriers: number;
	tokens: number;
	last_seen: string | null;
}

export interface AdminUserPage {
	items: AdminUserRow[];
	page: number;
	per_page: number;
	total: number;
	total_pages: number;
}

/** `GET /api/admin/users/{id}`: the row plus what the account holds. */
export interface AdminUserDetail extends AdminUserRow {
	active_sessions: number;
	carrier_list: {
		carrier_id: number;
		callsign: string | null;
		name: string | null;
		visibility: Visibility;
		updated_at: string;
	}[];
	token_list: {
		id: number;
		name: string;
		created_at: string;
		last_used_at: string | null;
	}[];
}
