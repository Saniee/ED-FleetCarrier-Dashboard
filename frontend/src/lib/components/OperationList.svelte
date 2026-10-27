<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/auth.svelte';
  import { formatNumber } from '$lib/format';
  import type { OperationSummary } from '$lib/types/operations';
  import OperationEditor from './OperationEditor.svelte';

  /** A carrier's hauling operations, on its page. The owner can start one. */
  let { callsign, owner }: { callsign: string; owner: boolean } = $props();

  let items = $state<OperationSummary[] | null>(null);
  let creating = $state(false);

  async function load() {
    const res = await api(`/api/carriers/${encodeURIComponent(callsign)}/operations`);
    items = res.ok ? await res.json() : [];
  }

  // Progress moves as haulers deliver; there is no stream for it, so re-read.
  $effect(() => {
    load();
    const timer = setInterval(load, 60_000);
    return () => clearInterval(timer);
  });
</script>

<div class="carrier_operations">
  <div class="history__head">
    <h2>Operations</h2>
    {#if owner}<button onclick={() => (creating = true)}>New operation</button>{/if}
  </div>

  {#if items === null}
    <p class="muted">Loading…</p>
  {:else if items.length === 0}
    <p class="muted">
      {owner
        ? 'No operations. Start one to track a hauling goal or a colonisation depot.'
        : 'No hauling operations running.'}
    </p>
  {:else}
    <ul class="operations">
      {#each items as item (item.operation.id)}
        <li>
          <a href={`/operation/${item.operation.id}`}>{item.operation.name}</a>
          {#if item.operation.status !== 'active'}<span class="tag">{item.operation.status}</span>{/if}
          {#if item.depot}
            <span class="muted">· {item.depot.station_name ?? 'depot'}, {item.depot.star_system ?? '—'}</span>
          {/if}
          <div class="meter" title={`${Math.round(item.fraction * 100)}%`}>
            <div class="meter__fill" style:width={`${Math.min(100, item.fraction * 100)}%`}></div>
          </div>
          <span class="stat__hint">{formatNumber(item.provided)} / {formatNumber(item.required)} t</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if creating}
  <OperationEditor
    {callsign}
    onsaved={(id) => goto(`/operation/${id}`)}
    onclose={() => (creating = false)}
  />
{/if}
