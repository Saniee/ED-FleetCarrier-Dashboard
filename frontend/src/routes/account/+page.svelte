<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { auth, api } from '$lib/auth.svelte';
  import type { CarrierListItem } from '$lib/types/carrier';
  import { formatTimestamp } from '$lib/format';
  import { isSquadronHull } from '$lib/carrier-fuel';
  import { VISIBILITIES, VISIBILITY_LABEL, type Visibility } from '$lib/visibility';

  interface ApiToken {
    id: number;
    name: string;
    created_at: string;
    last_used_at: string | null;
  }

  let tokens = $state<ApiToken[]>([]);
  let carriers = $state<CarrierListItem[]>([]);
  let tokenName = $state('plugin');
  let newSecret = $state('');
  let claimCallsign = $state('');
  let error = $state('');

  async function loadAll() {
    const [t, c] = await Promise.all([api('/api/auth/tokens'), api('/api/carriers/mine')]);
    if (t.status === 401 || c.status === 401) {
      await auth.logout();
      await goto('/login');
      return;
    }
    if (t.ok) tokens = (await t.json()) as ApiToken[];
    if (c.ok) carriers = (await c.json()) as CarrierListItem[];
  }

  async function createToken(e: Event) {
    e.preventDefault();
    error = '';
    const res = await api('/api/auth/tokens', {
      method: 'POST',
      body: JSON.stringify({ name: tokenName })
    });
    if (!res.ok) {
      error = `Could not create token (${res.status}).`;
      return;
    }
    newSecret = ((await res.json()) as { token: string }).token;
    await loadAll();
  }

  async function revoke(id: number) {
    await api(`/api/auth/tokens/${id}`, { method: 'DELETE' });
    await loadAll();
  }

  async function setVisibility(callsign: string | null, visibility: Visibility) {
    if (!callsign) return;
    error = '';
    const res = await api(`/api/carriers/${encodeURIComponent(callsign)}`, {
      method: 'PATCH',
      body: JSON.stringify({ visibility })
    });
    if (!res.ok) error = `Could not change visibility (${res.status}).`;
    await loadAll();
  }

  async function claim(e: Event) {
    e.preventDefault();
    error = '';
    const res = await api(`/api/carriers/${encodeURIComponent(claimCallsign.trim())}/claim`, {
      method: 'POST'
    });
    if (res.status === 404) error = 'No carrier with that callsign has been seen yet.';
    else if (res.status === 409) error = 'That carrier already belongs to someone else.';
    else if (!res.ok) error = `Claim failed (${res.status}).`;
    await loadAll();
  }

  onMount(() => {
    if (!auth.user) void goto('/login');
    else void loadAll();
  });
</script>

<svelte:head>
  <title>Account · ED Commander</title>
</svelte:head>

<div class="carrier-page">
  {#if auth.user}
    <div class="history__head">
      <h2>{auth.user.username}</h2>
    </div>
    {#if error}<p class="muted">{error}</p>{/if}

    <div class="history__head">
      <h2>My carriers</h2>
    </div>
    <p class="muted">
      A carrier is claimed automatically the first time the plugin sends its CarrierStats
      (open Carrier Management in game). You can also claim one by callsign.
    </p>
    <form onsubmit={claim}>
      <input bind:value={claimCallsign} placeholder="callsign" />
      <button type="submit">Claim</button>
    </form>
    {#if carriers.length === 0}
      <p class="muted">No carriers yet.</p>
    {:else}
      <div class="table-wrap">
        <table>
          <thead>
            <tr><th>Callsign</th><th>Name</th><th>System</th><th>Visibility</th></tr>
          </thead>
          <tbody>
            {#each carriers as c (c.carrier_id)}
              <tr>
                <td><a href={`/carrier/${encodeURIComponent(c.callsign ?? '')}`}>{c.callsign}</a></td>
                <td>{c.name ?? '—'}{#if c.is_squadron || isSquadronHull(c.carrier_type)} <span class="muted">[Squadron]</span>{/if}</td>
                <td>{c.star_system ?? '—'}</td>
                <td>
                  <select
                    value={c.visibility}
                    onchange={(e) => setVisibility(c.callsign, e.currentTarget.value as Visibility)}
                  >
                    {#each VISIBILITIES as v}
                      <option value={v}>{VISIBILITY_LABEL[v]}</option>
                    {/each}
                  </select>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    <div class="history__head">
      <h2>Plugin API tokens</h2>
    </div>
    <p class="muted">Paste a token into the EDMC plugin token setting.</p>
    <form onsubmit={createToken}>
      <input bind:value={tokenName} placeholder="token name" />
      <button type="submit">Create</button>
    </form>
    {#if newSecret}
      <p>Copy it now, it is shown only once: <input readonly value={newSecret} size="70" /></p>
    {/if}
    {#if tokens.length > 0}
      <div class="table-wrap">
        <table>
          <thead>
            <tr><th>Name</th><th>Created</th><th>Last used</th><th></th></tr>
          </thead>
          <tbody>
            {#each tokens as t (t.id)}
              <tr>
                <td>{t.name}</td>
                <td>{formatTimestamp(t.created_at)}</td>
                <td>{t.last_used_at ? formatTimestamp(t.last_used_at) : 'never'}</td>
                <td><button onclick={() => revoke(t.id)}>Revoke</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>
