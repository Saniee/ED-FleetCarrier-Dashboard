/**
 * Who can see a carrier.
 *
 *  - public:     listed on `/`, readable by anyone
 *  - private:    not listed, readable by anyone with the `/carrier/{callsign}` link
 *  - owner_only: readable only by the owner, who finds it on the account page
 *                (provisional name)
 */
export type Visibility = 'public' | 'private' | 'owner_only';

export const VISIBILITIES: Visibility[] = ['public', 'private', 'owner_only'];

export const VISIBILITY_LABEL: Record<Visibility, string> = {
	public: 'Public',
	private: 'Private (link only)',
	owner_only: 'Owner only'
};
