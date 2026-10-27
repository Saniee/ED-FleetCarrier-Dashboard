<script lang="ts">
  import { SITE_NAME } from '$lib/site';
  import { untrack } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/auth.svelte';
  import { formatNumber, formatTimestamp } from '$lib/format';
  import type { OperationDetail, OperationStatus } from '$lib/types/operations';
  import OperationEditor from '$lib/components/OperationEditor.svelte';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  let detail = $state<OperationDetail | null>(untrack(() => data.initial));
  let editing = $state(false);
  let confirmingDelete = $state(false);
  let error = $state('');

  const path = $derived(`/api/operations/${encodeURIComponent(data.id)}`);
  const op = $derived(detail?.operation);
  const percent = (f: number) => `${Math.min(100, Math.round(f * 100))}%`;

  async function load() {
    const res = await api(path);
    if (res.ok) detail = await res.json();
  }

  // Deliveries arrive without a stream; re-read while the page is open.
  $effect(() => {
    if (!detail) return;
    const timer = setInterval(load, 30_000);
    return () => clearInterval(timer);
  });

  async function setStatus(status: OperationStatus) {
    error = '';
    const res = await api(path, { method: 'PATCH', body: JSON.stringify({ status }) });
    if (!res.ok) error = `Could not update (${res.status}).`;
    await load();
  }

  async function remove() {
    const res = await api(path, { method: 'DELETE' });
    if (!res.ok) {
      error = `Could not delete (${res.status}).`;
      return;
    }
    goto(`/carrier/${encodeURIComponent(detail?.carrier.callsign ?? '')}`);
  }
</script>

<svelte:head>
  <title>{op ? `${op.name} · ${SITE_NAME}` : SITE_NAME}</title>
</svelte:head>

<div class="carrier-page">
  {#if !detail || !op}
    <h1>{data.notFound ? 'Operation not found' : 'Could not load the operation'}</h1>
  {:else}
    <header class="carrier_header">
      <h1>
        {op.name}
        {#if op.status !== 'active'}<span class="tag">{op.status}</span>{/if}
      </h1>
      {#if detail.can_manage}
        <div>
          <button onclick={() => (editing = true)}>Edit</button>
          {#if op.status === 'active'}
            <button onclick={() => setStatus('complete')}>Mark complete</button>
          {:else}
            <button onclick={() => setStatus('active')}>Reopen</button>
          {/if}
          {#if op.status !== 'archived'}
            <button onclick={() => setStatus('archived')}>Archive</button>
          {/if}
        </div>
      {/if}
    </header>

    <p class="market__meta">
      Carrier
      <a href={`/carrier/${encodeURIComponent(detail.carrier.callsign ?? '')}`}>
        {detail.carrier.name ?? 'unnamed carrier'} ({detail.carrier.callsign})
      </a>
      {#if detail.depot}
        · for {detail.depot.station_name ?? 'a construction site'}, {detail.depot.star_system ?? '—'}
        · depot read {formatTimestamp(detail.depot.updated_at)}
      {/if}
      · started {formatTimestamp(op.created_at)}
    </p>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="stats">
      <div class="stat">
        <span class="stat__label">Progress</span>
        <span class="stat__value">{percent(detail.progress.fraction)}</span>
        <span class="stat__hint">
          {formatNumber(detail.progress.provided)} / {formatNumber(detail.progress.required)} t
        </span>
      </div>
      <div class="stat">
        <span class="stat__label">Still needed</span>
        <span class="stat__value">
          {formatNumber(detail.progress.required - detail.progress.provided)} t
        </span>
      </div>
    </div>
    <div class="meter meter--wide">
      <div class="meter__fill" style:width={percent(detail.progress.fraction)}></div>
    </div>

    <div class="carrier_market">
      <h2>Targets</h2>
      {#if detail.progress.targets.length === 0}
        <p class="muted">
          {detail.depot ? 'The depot has not listed its needs yet.' : 'No targets yet.'}
        </p>
      {:else}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Commodity</th>
                <th>Progress</th>
                <th class="num">Needed</th>
                <th class="num">{detail.depot ? 'At depot' : 'Delivered'}</th>
                <th class="num">Remaining</th>
                <th class="num" title="In the carrier's market at its last read">On carrier</th>
              </tr>
            </thead>
            <tbody>
              {#each detail.progress.targets as t (t.commodity)}
                <tr>
                  <td>{t.name}</td>
                  <td>
                    <div class="meter">
                      <div class="meter__fill" style:width={percent(t.provided / t.required)}></div>
                    </div>
                  </td>
                  <td class="num">{formatNumber(t.required)}</td>
                  <td class="num">{formatNumber(t.provided)}</td>
                  <td class="num">{formatNumber(t.remaining)}</td>
                  <td class="num">{formatNumber(t.on_carrier)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <div class="operation_section">
      <h2>Haulers</h2>
      {#if detail.progress.contributors.length === 0}
        <p class="muted">
          No deliveries reported yet. Haulers running the plugin show up here when they sell to the
          carrier{detail.depot ? ' or deliver to the depot' : ''}.
        </p>
      {:else}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Commander</th>
                <th class="num">To carrier</th>
                <th class="num">To depot</th>
              </tr>
            </thead>
            <tbody>
              {#each detail.progress.contributors as c (c.user_id)}
                <tr>
                  <td>{c.username}</td>
                  <td class="num">{formatNumber(c.to_carrier)}</td>
                  <td class="num">{formatNumber(c.to_depot)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    {#if detail.can_manage}
      <div class="operation_section">
        {#if confirmingDelete}
          <div class="form__actions">
            <button class="button--danger" onclick={remove}>Confirm delete</button>
            <button onclick={() => (confirmingDelete = false)}>Keep</button>
          </div>
        {:else}
          <button onclick={() => (confirmingDelete = true)}>Delete operation</button>
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#if editing && detail && op}
  <OperationEditor
    callsign={detail.carrier.callsign ?? ''}
    id={op.id}
    name={op.name}
    depot={op.depot_market_id !== null}
    targets={detail.progress.targets.map((t) => ({ commodity: t.commodity, name: t.name, required: t.required }))}
    onsaved={load}
    onclose={() => (editing = false)}
  />
{/if}
