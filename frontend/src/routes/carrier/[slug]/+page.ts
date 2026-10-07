import type { PageLoad } from './$types';
import type { Carrier } from '$lib/types/carrier';
import { auth } from '$lib/auth.svelte';

export const load: PageLoad = async ({ fetch, params }) => {
  // Sent with the session so an owner_only carrier loads for its owner.
  const res = await fetch(`/api/carriers/${encodeURIComponent(params.slug)}`, {
    headers: auth.token ? { Authorization: `Bearer ${auth.token}` } : {}
  });
  // A 404 covers both a callsign that does not exist and an owner_only carrier
  // the viewer may not see. They are deliberately indistinguishable.
  if (res.status === 404) {
    return { initial: null, slug: params.slug, notFound: true };
  }
  if (!res.ok) {
    return { initial: null, slug: params.slug, notFound: false };
  }

  return { initial: (await res.json()) as Carrier, slug: params.slug, notFound: false };
};
