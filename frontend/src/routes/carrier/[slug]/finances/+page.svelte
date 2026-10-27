<script lang="ts">
  import { SITE_NAME } from '$lib/site';
  import { onMount, untrack } from 'svelte';
  import type { PageData } from './$types';
  import { api } from '$lib/auth.svelte';
  import { createCarrierFeed } from '$lib/carrier-feed.svelte';
  import { runway, nextUpkeep } from '$lib/carrier-finance';
  import { carrierMassFor, routeFuel, MAX_JUMP_RANGE } from '$lib/carrier-fuel';
  import { formatCredits, formatDay, formatNumber, formatTimestamp } from '$lib/format';
  import WeeklyChart from '$lib/components/WeeklyChart.svelte';

  let { data }: { data: PageData } = $props();

  // Live, so runway follows the balance as the plugin reports it.
  const feed = createCarrierFeed(
    untrack(() => data.initial),
    untrack(() => data.slug)
  );
  onMount(() => {
    if (!data.history) return;
    feed.start();
    return () => feed.stop();
  });

  const c = $derived(feed.carrier);
  const path = $derived(`/api/carriers/${encodeURIComponent(data.slug)}`);

  // ---------- upkeep and runway ----------

  let upkeep = $state<number | null>(untrack(() => data.initial?.weekly_upkeep ?? null));
  let saving = $state(false);
  let error = $state('');

  const now = Date.now();
  const left = $derived(runway(c?.carrier_balance ?? null, c?.weekly_upkeep ?? null, now));

  async function saveUpkeep(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = '';
    const res = await api(path, {
      method: 'PATCH',
      body: JSON.stringify({ weekly_upkeep: Math.max(0, Math.round(upkeep ?? 0)) })
    });
    saving = false;
    // The feed brings the saved figure back with the carrier row.
    if (!res.ok) error = `Could not save the upkeep (${res.status}).`;
  }

  // ---------- history ----------

  const weeks = $derived(data.history?.weeks ?? []);
  const labels = $derived(weeks.map((w) => formatDay(w.start)));

  // ---------- route tritium ----------

  let destination = $state('');
  let looking = $state(false);
  let routeError = $state('');
  let target = $state<{ name: string; coords: { x: number; y: number; z: number } } | null>(null);

  async function lookUp(e: SubmitEvent) {
    e.preventDefault();
    const name = destination.trim();
    if (!name) return;
    looking = true;
    routeError = '';
    const res = await api(`/api/systems?name=${encodeURIComponent(name)}`);
    looking = false;
    if (res.ok) {
      target = await res.json();
    } else {
      target = null;
      routeError =
        res.status === 404
          ? `EDSM doesn't know ${name}, or has no coordinates for it.`
          : `Could not look the system up (${res.status}).`;
    }
  }

  const route = $derived.by(() => {
    if (!c || !target || c.star_x === null || c.star_y === null || c.star_z === null) return null;
    const distance = Math.hypot(target.coords.x - c.star_x, target.coords.y - c.star_y, target.coords.z - c.star_z);
    const used = Math.max(0, (c.space_total_capacity ?? 0) - (c.space_free ?? 0));
    // The current range already allows for a heavy hold.
    const range = c.jump_range_curr || MAX_JUMP_RANGE;
    return { distance, range, ...routeFuel(distance, range, used, c.fuel_level ?? 0, carrierMassFor(c.carrier_type)) };
  });
</script>

<svelte:head>
  <title>Finances · {c?.callsign ?? data.slug} · {SITE_NAME}</title>
</svelte:head>

<div class="carrier-page">
  {#if data.notFound || !c}
    <h1>Carrier not found</h1>
  {:else if data.denied || !data.history}
    <h1>Finances</h1>
    <p class="muted">
      {data.denied ? "Only the carrier's owner can see its finances." : 'Could not load the finances.'}
      <a href={`/carrier/${encodeURIComponent(data.slug)}`}>Back to the carrier</a>
    </p>
  {:else}
    <header class="carrier_header">
      <h1>
        Finances
        <span class="callsign">{c.name ?? 'unnamed carrier'} · {c.callsign}</span>
      </h1>
      <div>
        <a class="button" href={`/carrier/${encodeURIComponent(data.slug)}`}>Carrier</a>
      </div>
    </header>

    <div class="stats">
      <div class="stat">
        <span class="stat__label">Balance</span>
        {#key c.carrier_balance}
          <span class="stat__value stat__value--live">{formatCredits(c.carrier_balance)} CR</span>
        {/key}
        <span class="stat__hint">
          {formatCredits(c.reserve_balance)} reserved · {formatCredits(c.available_balance)} available
        </span>
      </div>
      <div class="stat">
        <span class="stat__label">Runway</span>
        {#if left}
          <span class="stat__value">{formatNumber(left.weeks)} week{left.weeks === 1 ? '' : 's'}</span>
          <span class="stat__hint">Short of upkeep on {formatDay(left.shortAt)}</span>
        {:else}
          <span class="stat__value">—</span>
          <span class="stat__hint">Enter the weekly upkeep below.</span>
        {/if}
      </div>
      <div class="stat">
        <span class="stat__label">Next upkeep</span>
        <span class="stat__value">{formatTimestamp(new Date(nextUpkeep(now)).toISOString())}</span>
        {#if c.weekly_upkeep}
          <span class="stat__hint">{formatCredits(c.weekly_upkeep)} CR a week</span>
        {/if}
      </div>
    </div>

    <form onsubmit={saveUpkeep}>
      <label for="upkeep" class="stat__label">Weekly upkeep</label>
      <input id="upkeep" type="number" min="0" step="1" bind:value={upkeep} placeholder="Credits, from the in-game finance screen" />
      <button disabled={saving}>Save</button>
    </form>
    <p class="muted">
      The game doesn't report upkeep to the journal. Copy it from carrier management → Finance; 0
      clears it. A Discord alert for a low balance can be switched on under Manage.
    </p>
    {#if error}<p class="error">{error}</p>{/if}

    <div class="operation_section">
      <h2>Week by week</h2>
      <p class="muted">
        How the balance moved each upkeep week (from Thursday 07:00 UTC), leaving out your own
        deposits and withdrawals: market and service income, less upkeep, jump fees and buy orders.
        The game reports the balance only when you open carrier management, change tax or move
        credits.
      </p>
      {#if weeks.some((w) => w.earned !== null)}
        <WeeklyChart {labels} values={weeks.map((w) => w.earned)} />
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Week of</th>
                <th class="num">Change</th>
                <th class="num">Your transfers</th>
                <th class="num">Closing balance</th>
              </tr>
            </thead>
            <tbody>
              {#each [...weeks].reverse() as w (w.start)}
                <tr>
                  <td>{formatDay(w.start)}</td>
                  <td class="num" title={w.spans_weeks ? 'Includes earlier weeks with no report' : undefined}>
                    {w.earned === null ? '—' : formatCredits(w.earned)}{w.spans_weeks ? '*' : ''}
                  </td>
                  <td class="num">{w.transferred ? formatCredits(w.transferred) : '—'}</td>
                  <td class="num">{w.closing === null ? '—' : formatCredits(w.closing)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if weeks.some((w) => w.spans_weeks)}
          <p class="muted">* Includes the weeks before it that had no report of their own.</p>
        {/if}
      {:else}
        <p class="muted">No balance reports in the last {weeks.length} weeks.</p>
      {/if}
    </div>

    {#if data.history.transfers.length}
      <div class="operation_section">
        <h2>Transfers</h2>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>When</th>
                <th class="num">Amount</th>
                <th class="num">Balance after</th>
              </tr>
            </thead>
            <tbody>
              {#each data.history.transfers as t (t.at)}
                <tr>
                  <td>{formatTimestamp(t.at)}</td>
                  <td class="num">{t.amount > 0 ? '+' : ''}{formatCredits(t.amount)}</td>
                  <td class="num">{formatCredits(t.balance)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}

    <div class="operation_section">
      <h2>Tritium for a route</h2>
      <form onsubmit={lookUp}>
        <input bind:value={destination} placeholder="Destination system" autocomplete="off" />
        <button disabled={looking || !destination.trim()}>Estimate</button>
      </form>
      {#if routeError}<p class="error">{routeError}</p>{/if}
      {#if target && !route}
        <p class="muted">The carrier's position isn't known yet; it arrives with its next jump.</p>
      {:else if target && route}
        <div class="stats">
          <div class="stat">
            <span class="stat__label">To {target.name}</span>
            <span class="stat__value">{formatNumber(Math.round(route.distance))} ly</span>
            <span class="stat__hint">
              {formatNumber(route.jumps)} jump{route.jumps === 1 ? '' : 's'} of up to {formatNumber(route.range)} ly
            </span>
          </div>
          <div class="stat">
            <span class="stat__label">Tritium</span>
            <span class="stat__value">{formatNumber(route.total)} t</span>
            <span class="stat__hint">{formatNumber(c.fuel_level)} t in the tank</span>
          </div>
          <div class="stat">
            <span class="stat__label">{route.short ? 'Short by' : 'Left on arrival'}</span>
            <span class="stat__value">
              {formatNumber(route.short || (c.fuel_level ?? 0) - route.total)} t
            </span>
            {#if route.short}<span class="stat__hint">Load it into the hold before you go.</span>{/if}
          </div>
        </div>
        <p class="muted">
          A straight-line estimate at today's cargo; a plotted route usually needs a jump or two more.
        </p>
      {/if}
    </div>
  {/if}
</div>
