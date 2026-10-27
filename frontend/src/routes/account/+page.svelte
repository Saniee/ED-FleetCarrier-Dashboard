<script lang="ts">
  import { SITE_NAME } from '$lib/site';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { auth, api } from '$lib/auth.svelte';
  import type { CarrierListItem } from '$lib/types/carrier';
  import { formatTimestamp } from '$lib/format';
  import { isSquadronHull } from '$lib/carrier-fuel';
  import { VISIBILITY_LABEL } from '$lib/visibility';
  import CarrierSettings from '$lib/components/CarrierSettings.svelte';
  import RegenerateToken from '$lib/components/RegenerateToken.svelte';
  import PluginDownload from '$lib/components/PluginDownload.svelte';
  import { motion } from '$lib/motion.svelte';

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
  // Which token the last secret belongs to, for the copy prompt.
  let secretFor = $state('');
  let regenerating = $state<ApiToken | null>(null);
  let error = $state('');
  // Callsign whose settings dialog is open. `?manage=` opens one on arrival
  // (the carrier page's Manage button links here).
  let managing = $state<string | null>(page.url.searchParams.get('manage'));
  const managed = $derived(carriers.find((c) => c.callsign === managing));

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
    secretFor = tokenName;
    await loadAll();
  }

  async function revoke(id: number) {
    await api(`/api/auth/tokens/${id}`, { method: 'DELETE' });
    await loadAll();
  }

  onMount(() => {
    if (!auth.user) void goto('/login');
    else void loadAll();
  });
</script>

<svelte:head>
  <title>Account · {SITE_NAME}</title>
</svelte:head>

<div class="carrier-page">
  {#if auth.user}
    <section class="section">
      <div class="history__head">
        <h2>{auth.user.username}</h2>
      </div>
      {#if error}<p class="error">{error}</p>{/if}
    </section>

    <section class="section">
      <div class="history__head">
        <h2>My carriers</h2>
      </div>
      <p class="muted">
        A carrier is claimed automatically the first time the plugin sends its CarrierStats
        (open Carrier Management in game).
      </p>
      {#if carriers.length === 0}
        <p class="muted">No carriers yet.</p>
      {:else}
        <div class="table-wrap">
          <table>
            <thead>
              <tr><th>Callsign</th><th>Name</th><th>System</th><th>Visibility</th><th></th></tr>
            </thead>
            <tbody>
              {#each carriers as c (c.carrier_id)}
                <tr>
                  <td><a href={`/carrier/${encodeURIComponent(c.callsign ?? '')}`}>{c.callsign}</a></td>
                  <td>{c.name ?? '—'}{#if c.is_squadron || isSquadronHull(c.carrier_type)} <span class="tag">Squadron</span>{/if}</td>
                  <td>{c.star_system ?? '—'}</td>
                  <td>
                    {c.visibility ? VISIBILITY_LABEL[c.visibility] : '—'}
                    {#if c.moderation_hidden_at}
                      <span class="tag tag--alert" title={c.moderation_reason ?? 'No reason given'}>Hidden by a moderator</span>
                    {/if}
                  </td>
                  <td class="actions">
                    <button onclick={() => (managing = c.callsign)}>Manage</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      {#if managed}
        <CarrierSettings
          carrier={managed}
          onchange={loadAll}
          onclose={() => (managing = null)}
        />
      {/if}
    </section>

    <PluginDownload />

    <section class="section">
      <div class="history__head">
        <h2>Plugin API tokens</h2>
      </div>
      <p class="muted">Paste a token into the EDMC plugin token setting.</p>
      <form onsubmit={createToken}>
        <input bind:value={tokenName} placeholder="token name" />
        <button type="submit">Create</button>
      </form>
      {#if newSecret}
        <div class="secret">
          <p>New secret for {secretFor}. Copy it now, it is shown only once:</p>
          <input readonly value={newSecret} onfocus={(e) => e.currentTarget.select()} />
        </div>
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
                  <td class="actions">
                    <button onclick={() => (regenerating = t)}>Regenerate</button>
                    <button onclick={() => revoke(t.id)}>Revoke</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
      {#if regenerating}
        <RegenerateToken
          token={regenerating}
          ondone={(secret) => {
            newSecret = secret;
            secretFor = regenerating?.name ?? '';
            void loadAll();
          }}
          onclose={() => (regenerating = null)}
        />
      {/if}
    </section>

    <section class="section">
      <div class="history__head">
        <h2>Preferences</h2>
      </div>
      <label class="check">
        <input
          type="checkbox"
          checked={motion.full}
          onchange={(e) => motion.set(e.currentTarget.checked)}
        />
        Play animations even with reduced motion on
      </label>
      <p class="muted">
        Saved in this browser.
        {motion.systemReduced
          ? 'Your device currently asks for reduced motion.'
          : 'Your device is not asking for reduced motion, so animations already play.'}
      </p>
    </section>
  {/if}
</div>
