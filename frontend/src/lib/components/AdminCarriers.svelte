<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/auth.svelte';
  import { formatTimestamp } from '$lib/format';
  import type { AdminCarrierPage } from '$lib/types/admin';
  import { VISIBILITY_LABEL } from '$lib/visibility';

  /**
   * Every carrier, whatever its visibility. A new `version` re-reads the
   * current page; `onmanage` opens a carrier, `onopenuser` its owner.
   */
  let {
    version,
    onmanage,
    onopenuser
  }: {
    version: number;
    onmanage: (id: number) => void;
    onopenuser: (id: number) => void;
  } = $props();

  const PER_PAGE = 25;
  const FILTERS = { all: 'All', hidden: 'Hidden', unowned: 'Unowned' } as const;

  let result = $state<AdminCarrierPage | null>(null);
  let page = $state(1);
  let query = $state('');
  let filter = $state<keyof typeof FILTERS>('all');
  let error = $state('');
  let seq = 0;

  async function load() {
    const mine = ++seq;
    error = '';
    const params = new URLSearchParams({ page: String(page), per_page: String(PER_PAGE), filter });
    if (query.trim()) params.set('q', query.trim());
    const res = await api(`/api/admin/carriers?${params}`).catch(() => null);
    if (mine !== seq) return;
    if (res?.ok) result = (await res.json()) as AdminCarrierPage;
    else error = 'Could not load carriers.';
  }

  function search(e: Event) {
    e.preventDefault();
    page = 1;
    void load();
  }

  function go(next: number) {
    page = next;
    void load();
  }

  // First load, then again whenever something changed elsewhere.
  $effect(() => {
    void version;
    untrack(load);
  });
</script>

<section class="section">
  <div class="history__head">
    <h2>Carriers</h2>
    {#if result}<span class="status status--online">{result.total} total</span>{/if}
  </div>

  <form onsubmit={search}>
    <input bind:value={query} placeholder="callsign, name, system or owner" />
    <select bind:value={filter} onchange={search}>
      {#each Object.entries(FILTERS) as [key, label] (key)}
        <option value={key}>{label}</option>
      {/each}
    </select>
    <button type="submit">Search</button>
  </form>

  {#if error}
    <p class="error">{error}</p>
  {:else if !result}
    <p class="muted">Loading…</p>
  {:else if result.items.length === 0}
    <p class="muted">No carriers found.</p>
  {:else}
    <div class="table-wrap">
      <table class="admin-carriers">
        <thead>
          <tr><th>Callsign</th><th>Name</th><th>Owner</th><th>Visibility</th><th>Updated</th><th></th></tr>
        </thead>
        <tbody>
          {#each result.items as c (c.carrier_id)}
            <tr>
              <td>
                {#if c.callsign && c.visibility !== 'owner_only'}
                  <a href={`/carrier/${encodeURIComponent(c.callsign)}`}>{c.callsign}</a>
                {:else}
                  {c.callsign ?? `#${c.carrier_id}`}
                {/if}
                {#if c.moderation_hidden_at}<span class="tag tag--alert">Hidden</span>{/if}
              </td>
              <td>{c.name ?? '—'}</td>
              <td>
                {#if c.owner_id !== null}
                  <button class="link" onclick={() => onopenuser(c.owner_id!)}>{c.owner}</button>
                  {#if c.owner_disabled}<span class="tag tag--alert">Disabled</span>{/if}
                {:else}
                  <span class="muted">unowned</span>
                {/if}
              </td>
              <td>{VISIBILITY_LABEL[c.visibility]}</td>
              <td>{c.updated_at ? formatTimestamp(c.updated_at) : '—'}</td>
              <td class="actions"><button onclick={() => onmanage(c.carrier_id)}>Manage</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <p class="pager">
      <button disabled={result.page <= 1} onclick={() => go(result!.page - 1)}>Prev</button>
      <span>Page {result.page} of {Math.max(1, result.total_pages)}</span>
      <button disabled={result.page >= result.total_pages} onclick={() => go(result!.page + 1)}>Next</button>
    </p>
  {/if}
</section>
