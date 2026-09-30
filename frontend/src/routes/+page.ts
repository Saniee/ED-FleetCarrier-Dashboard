import type { PageLoad } from './$types';
import type { Carrier } from '$lib/types/carrier';

export const ssr = false;

export const load: PageLoad = async ({ fetch }) => {
  const res = await fetch('/api/carrier');
  if (!res.ok) {
    return { initial: null };
  }

  return { initial: (await res.json()) as Carrier };
};
