/** Types for the hauling operations API (`/api/operations`, `/api/carriers/{callsign}/operations`). */

export type OperationStatus = 'active' | 'complete' | 'archived';

export interface Operation {
	id: number;
	carrier_id: number;
	name: string;
	/** Set for a colonisation operation: the targets are the depot's needs. */
	depot_market_id: number | null;
	status: OperationStatus;
	created_at: string;
	completed_at: string | null;
}

export interface DepotResource {
	commodity: string;
	name: string;
	required: number;
	provided: number;
	payment: number;
}

export interface Depot {
	market_id: number;
	star_system: string | null;
	station_name: string | null;
	/** 0..1 */
	progress: number;
	complete: boolean;
	failed?: boolean;
	resources?: DepotResource[];
	updated_at: string;
}

export interface TargetProgress {
	commodity: string;
	name: string;
	required: number;
	provided: number;
	remaining: number;
	/** In the carrier's market at its last read. */
	on_carrier: number;
	to_carrier: number;
	to_depot: number;
}

export interface Contributor {
	user_id: number;
	username: string;
	to_carrier: number;
	to_depot: number;
}

export interface Progress {
	targets: TargetProgress[];
	contributors: Contributor[];
	required: number;
	provided: number;
	/** 0..1 */
	fraction: number;
	met: boolean;
}

/** `GET /api/operations/{id}` */
export interface OperationDetail {
	operation: Operation;
	carrier: { callsign: string | null; name: string | null };
	depot: Depot | null;
	progress: Progress;
	can_manage: boolean;
}

/** One entry of `GET /api/carriers/{callsign}/operations` */
export interface OperationSummary {
	operation: Operation;
	depot: { star_system: string | null; station_name: string | null } | null;
	required: number;
	provided: number;
	fraction: number;
}

export interface KnownCommodity {
	commodity: string;
	name: string;
}

/** A target as the editor sends it. */
export interface TargetInput {
	commodity: string;
	name?: string;
	required: number;
}
