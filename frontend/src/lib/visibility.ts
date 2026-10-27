/**
 * Who can see a carrier: `public` (listed, readable by anyone), `private`
 * (unlisted, readable with the link), `owner_only` (readable only by the owner).
 */
export type Visibility = 'public' | 'private' | 'owner_only';

export const VISIBILITIES: Visibility[] = ['public', 'private', 'owner_only'];

export const VISIBILITY_LABEL: Record<Visibility, string> = {
	public: 'Public',
	private: 'Private (link only)',
	owner_only: 'Owner only'
};
