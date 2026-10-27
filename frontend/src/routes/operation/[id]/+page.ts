import type { PageLoad } from './$types';
import type { OperationDetail } from '$lib/types/operations';
import { auth } from '$lib/auth.svelte';

export const load: PageLoad = async ({ fetch, params }) => {
  // With the session, so an owner_only carrier's operation loads for its owner.
  const res = await fetch(`/api/operations/${encodeURIComponent(params.id)}`, {
    headers: auth.token ? { Authorization: `Bearer ${auth.token}` } : {}
  });
  return {
    id: params.id,
    initial: res.ok ? ((await res.json()) as OperationDetail) : null,
    notFound: res.status === 404
  };
};
