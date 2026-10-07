<script lang="ts">
  import { onMount } from 'svelte';
  import type { CarrierPage } from '$lib/types/carrier';
  import { formatTimestamp } from '$lib/format';

  const PER_PAGE = 20;

  let result = $state<CarrierPage | null>(null);
  let page = $state(1);
  let query = $state('');
  let error = $state('');

  // Guards against a slow response landing after a newer one.
  let seq = 0;

  async function load() {
    const mine = ++seq;
    error = '';
    const params = new URLSearchParams({ page: String(page), per_page: String(PER_PAGE) });
    if (query.trim()) params.set('q', query.trim());
    try {
      const res = await fetch(`/api/carriers?${params}`);
      if (!res.ok) throw new Error(String(res.status));
      const next = (await res.json()) as CarrierPage;
      if (mine === seq) result = next;
    } catch {
      if (mine === seq) error = 'Could not load carriers.';
    }
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

  onMount(load);
</script>

<svelte:head>
  <title>Carriers · ED Commander</title>
</svelte:head>

<div class="carrier-page">
  <div class="history__head">
    <h2>Carriers</h2>
    {#if result}<span class="status status--online">{result.total} listed</span>{/if}
  </div>

  <form onsubmit={search}>
    <input bind:value={query} placeholder="callsign, name or system" />
    <button type="submit">Search</button>
  </form>

  {#if error}
    <p class="muted">{error}</p>
  {:else if !result}
    <p class="muted">Loading…</p>
  {:else if result.items.length === 0}
    <p class="muted">No carriers found.</p>
  {:else}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Callsign</th>
            <th>Name</th>
            <th>System</th>
            <th>Docking</th>
            <th>Updated</th>
          </tr>
        </thead>
        <tbody>
          {#each result.items as c (c.carrier_id)}
            <tr>
              <td><a href={`/carrier/${encodeURIComponent(c.callsign ?? '')}`}>{c.callsign}</a></td>
              <td>{c.name ?? '—'}</td>
              <td>{c.star_system ?? '—'}{#if c.location_unverified}*{/if}</td>
              <td>{c.docking_access ?? '—'}</td>
              <td>{formatTimestamp(c.updated_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <p>
      <button disabled={result.page <= 1} onclick={() => go(result!.page - 1)}>Prev</button>
      Page {result.page} of {Math.max(1, result.total_pages)}
      <button disabled={result.page >= result.total_pages} onclick={() => go(result!.page + 1)}>Next</button>
    </p>
  {/if}
</div>
