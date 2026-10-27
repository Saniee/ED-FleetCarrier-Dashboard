<script lang="ts">
  import { SITE_NAME } from '$lib/site';
  import { onMount, untrack } from 'svelte';
  import type { PageData } from './$types';
  import { createCarrierFeed } from '$lib/carrier-feed.svelte';
  import { eventSummary } from '$lib/carrier-summary';
  import { formatTimestamp, formatNumber, formatCountdown, parseTimestamp } from '$lib/format';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import { carrierMassFor, isSquadronHull, fuelPerJump, jumpsLeft, MAX_JUMP_RANGE } from '$lib/carrier-fuel';
  import { commodityName, commodityCategory, commodityPrice, commodityQuantity, isTraded } from '$lib/commodity';

  let { data }: { data: PageData } = $props();

  // Stream, reconnect/backoff, liveness probe and history all live in the feed
  // module. Reading feed.carrier / feed.events / feed.status in the template is
  // what subscribes it.
  const feed = createCarrierFeed(
    untrack(() => data.initial),
    untrack(() => data.slug)
  );

  let now = $state(Date.now());

  // The scheduled jump is denormalized onto the carrier row, so this comes
  // straight from the SSE snapshot — no need to inspect the event itself.
  // CarrierJumpRequest fills these; CarrierJumpCancelled nulls them.
  const jump = $derived.by(() => {
    const system = feed.carrier?.jump_destination_system;
    const departure = feed.carrier?.jump_departure_time;
    if (!system || !departure) return null;

    const at = parseTimestamp(departure);
    return at === null ? null : { system, at };
  });

  // Tritium estimate. The carrier's own hull mass dominates the cost, so this
  // stays close even with an empty hold. Distance is assumed to be a full
  // max-range jump, which yields the conservative (fewest) jumps.
  const fuel = $derived.by(() => {
    const c = feed.carrier;
    if (!c || c.fuel_level === null) return null;

    const capacityUsed = Math.max(0, (c.space_total_capacity ?? 0) - (c.space_free ?? 0));
    const perJump = fuelPerJump(MAX_JUMP_RANGE, capacityUsed, c.fuel_level, carrierMassFor(c.carrier_type));

    return { perJump, left: jumpsLeft(c.fuel_level, perJump) };
  });

  // A carrier's orders stay in `Market.json` after the quantity behind them runs
  // out, so the file can list a commodity that is not actually being traded. The
  // in-game market hides those, and so does this.
  const market = $derived.by(() => {
    const snapshot = feed.market;
    if (!snapshot) return null;

    return { event: snapshot.event, listed: snapshot.commodities.filter(isTraded) };
  });

  // Run the clock only while something is counting down. `now` is written but
  // never read here, so this cannot loop.
  $effect(() => {
    if (!jump) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  onMount(() => {
    if (data.notFound) return;
    feed.start();
    return () => feed.stop();
  });
  // Owner settings live in a dialog on the account page; this only decides
  // whether to link there. The backend enforces ownership.
  const owner = $derived(
    feed.carrier?.owner_id != null && feed.carrier.owner_id === auth.user?.id
  );
</script>
<svelte:head>
  <title>{feed.carrier?.callsign ?? data.slug} · {SITE_NAME}</title>
</svelte:head>

{#if data.notFound}
<div class="carrier-page">
  <h1>Carrier not found</h1>
</div>
{:else}
<div class="carrier-page">
  <div class="carrier_info">
    {#if feed.carrier}
      <header class="carrier_header">
        <h1>
        {feed.carrier.name ?? 'unnamed carrier'}
        <span class="callsign">{feed.carrier.callsign ?? '—'}</span>
        {#if feed.carrier.is_squadron || isSquadronHull(feed.carrier.carrier_type)}<span class="tag">Squadron</span>{/if}
      </h1>
      <div>
        {#if owner}
          <button onclick={() => goto(`/account?manage=${encodeURIComponent(feed.carrier?.callsign ?? '')}`)}>
            Manage
          </button>
        {/if}
        {#if auth.isAdmin}
          <button onclick={() => goto(`/admin?tab=carriers&carrier=${feed.carrier?.carrier_id}`)}>Moderate</button>
        {/if}
      </div>
    </header>
    {#if feed.carrier.visibility === 'private'}<p class="muted">Private: not in the carrier list, reachable by link.</p>{/if}
    {#if feed.carrier.visibility === 'owner_only'}<p class="muted">Owner only: nobody else can see this carrier.</p>{/if}
    {#if feed.carrier.moderation_hidden_at}
      <p class="error">
        Hidden by a moderator{feed.carrier.moderation_reason ? `: ${feed.carrier.moderation_reason}` : ''}.
        Only {owner ? 'you' : 'the owner'} and admins can see this carrier.
      </p>
    {/if}
      <div class="stats">
        <div class="stat">
          <span class="stat__label">Fuel</span>
          <!-- Keyed so a live change recreates the value and replays its flash. -->
          {#key feed.carrier.fuel_level}
            <span class="stat__value stat__value--live">{formatNumber(feed.carrier.fuel_level)}</span>
          {/key}
          {#if fuel}
            <span class="stat__hint">
              ≈ {formatNumber(fuel.left)} jumps left · {formatNumber(fuel.perJump)} t per
              {formatNumber(MAX_JUMP_RANGE)} ly
            </span>
          {/if}
        </div>
        <div class="stat">
          <span class="stat__label">Current system</span>
          {#key feed.carrier.star_system}
            <span class="stat__value stat__value--live">{feed.carrier.star_system ?? '—'}{#if feed.carrier.location_unverified}<span title="Last reported by someone other than the owner">*</span>{/if}</span>
          {/key}
          {#if feed.carrier.body}
            <span class="stat__hint">{feed.carrier.body}</span>
          {/if}
        </div>
      </div>
    {:else}
      <h1>No carrier yet.</h1>
    {/if}
  </div>

  <div class="jump_imminent">
    {#if jump}
      {#if jump.at > now}
        <p class="jump jump--active">
          Jumping to <strong>{jump.system}</strong> in {formatCountdown(jump.at - now)}
        </p>
      {:else}
        <p class="jump jump--active">
          Jumping to <strong>{jump.system}</strong> — in transit…
        </p>
      {/if}
    {:else}
      <p class="jump">Stationary</p>
    {/if}
  </div>

  <div class="carrier_market">
    <div class="history__head">
      <h2>Commodities</h2>
      {#if market}
        <span class="status status--online">
          {formatNumber(market.listed.length)} listed
        </span>
      {/if}
    </div>

    {#if market}
      <p class="market__meta">
        {market.event.station_name ?? '—'} · {market.event.star_system ?? '—'} · read
        {formatTimestamp(market.event.timestamp ?? market.event.updated_at)}
      </p>

      {#if market.listed.length > 0}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Commodity</th>
                <th>Category</th>
                <th class="num" title="What a visitor pays to buy this from the carrier — the carrier's export">Sells</th>
                <th class="num" title="What the carrier pays to buy this from a visitor — the carrier's import">Buys</th>
                <th class="num">Stock</th>
                <th class="num">Demand</th>
              </tr>
            </thead>
            <tbody>
              {#each market.listed as item (item.commodity_id)}
                <tr>
                  <td>{commodityName(item)}</td>
                  <td>{commodityCategory(item)}</td>
                  <td class="num">{commodityPrice(item.buy_price)}</td>
                  <td class="num">{commodityPrice(item.sell_price)}</td>
                  <td class="num">{commodityQuantity(item.stock)}</td>
                  <td class="num">{commodityQuantity(item.demand)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="muted">Nothing listed on the market.</p>
      {/if}
    {:else}
      <p class="muted">No market data yet — open the carrier's market in game.</p>
    {/if}
  </div>

  <div class="carrier_history">
    <div class="history__head">
      <h2>History</h2>
      <span
        class="status status--stream"
        class:status--online={feed.status === 'connected'}
        class:status--connecting={feed.status === 'connecting'}
        class:status--offline={feed.status === 'OFFLINE'}
      >{feed.status}</span>
    </div>

    {#if feed.events}
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>When</th>
              <th>Event</th>
              <th>Detail</th>
            </tr>
          </thead>
          <tbody>
            {#each feed.events as row (row.id)}
              <tr>
                <td>{formatTimestamp(row.timestamp)}</td>
                <td>{row.event_name}</td>
                <td>{eventSummary(row)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="muted">Loading history…</p>
    {/if}
  </div>
</div>
{/if}
