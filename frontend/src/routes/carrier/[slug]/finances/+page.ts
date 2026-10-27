import type { PageLoad } from './$types';
import type { Carrier } from '$lib/types/carrier';
import type { FinanceHistory } from '$lib/types/finances';
import { auth } from '$lib/auth.svelte';

export const load: PageLoad = async ({ fetch, params }) => {
  const headers: Record<string, string> = auth.token ? { Authorization: `Bearer ${auth.token}` } : {};
  const base = `/api/carriers/${encodeURIComponent(params.slug)}`;
  const [carrier, history] = await Promise.all([
    fetch(base, { headers }),
    fetch(`${base}/finances`, { headers })
  ]);
  return {
    slug: params.slug,
    initial: carrier.ok ? ((await carrier.json()) as Carrier) : null,
    history: history.ok ? ((await history.json()) as FinanceHistory) : null,
    // 401/403: signed out, or not the owner. The carrier itself may be public.
    denied: history.status === 401 || history.status === 403,
    notFound: carrier.status === 404
  };
};
