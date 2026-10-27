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

/** `GET /api/admin/settings`. `setting` is null when the environment applies. */
export interface AdminSettings {
	registration: { open: boolean; setting: boolean | null; env: boolean };
}

/** A row of `GET /api/admin/audit`. */
export interface AuditEntry {
	id: number;
	action: string;
	created_at: string;
	/** Null for the command line, or once the acting admin is deleted. */
	admin_id: number | null;
	admin: string | null;
	target_kind: 'user' | 'carrier' | null;
	target_id: number | null;
	/** Username or callsign: the current one, else the one recorded. */
	target: string | null;
	detail: Record<string, unknown>;
}

export interface AuditPage {
	items: AuditEntry[];
	page: number;
	per_page: number;
	total: number;
	total_pages: number;
}

/** A row of `GET /api/admin/carriers`: every carrier, whatever its visibility. */
export interface AdminCarrierRow {
	carrier_id: number;
	callsign: string | null;
	name: string | null;
	star_system: string | null;
	visibility: Visibility;
	updated_at: string | null;
	owner_id: number | null;
	owner: string | null;
	owner_disabled: boolean | null;
	moderation_hidden_at: string | null;
	moderation_reason: string | null;
}

export interface AdminCarrierPage {
	items: AdminCarrierRow[];
	page: number;
	per_page: number;
	total: number;
	total_pages: number;
}

/** `GET /api/admin/carriers/{id}`: the row plus what it stores. */
export interface AdminCarrierDetail extends AdminCarrierRow {
	events: number;
	commodities: number;
}

/** What the audit log is narrowed to. */
export interface AuditFilter {
	kind: 'user' | 'carrier';
	id: number;
	name: string;
}

/** `GET /api/admin/stats?days=`. Days are UTC `YYYY-MM-DD`. */
export interface AdminStats {
	days: number;
	/** First day of the range. */
	since: string;
	database_bytes: number;
	/** Biggest first. `rows` is Postgres' estimate. */
	tables: { name: string; rows: number; bytes: number }[];
	ingest_daily: {
		day: string;
		kind: 'carrier' | 'market' | 'hauling';
		outcome: 'stored' | 'ignored' | 'refused';
		count: number;
	}[];
	/** Most uploads first. `user_id` 0 is the legacy token or anonymous ingest. */
	reporters: {
		user_id: number;
		username: string | null;
		stored: number;
		ignored: number;
		refused: number;
		last_day: string;
	}[];
	/** Most events first. `recent` counts the events received in the range. */
	carriers: {
		carrier_id: number;
		callsign: string | null;
		name: string | null;
		events: number;
		recent: number;
		bytes: number;
		last_received: string | null;
	}[];
	accounts: {
		total: number;
		admins: number;
		disabled: number;
		/** Accounts whose plugin uploaded in the range. */
		reporting: number;
		/** Accounts that logged in during the range. */
		signed_in: number;
		live_sessions: number;
		signups_daily: { day: string; count: number }[];
	};
	carrier_counts: { total: number; owned: number; hidden: number; with_market: number };
}
