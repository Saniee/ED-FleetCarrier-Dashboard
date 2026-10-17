<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/auth.svelte';
  import { formatNumber, formatTimestamp } from '$lib/format';
  import type { AdminStats } from '$lib/types/admin';
  import BarChart, { type Series } from './BarChart.svelte';

  /**
   * Traffic and storage: what the plugins send, who sends it, and what the
   * database holds. `onopenuser` / `onopencarrier` open the moderation dialogs.
   */
  let {
    version,
    onopenuser,
    onopencarrier
  }: {
    version: number;
    onopenuser: (id: number) => void;
    onopencarrier: (id: number) => void;
  } = $props();

  const RANGES = [7, 30, 90, 365];

  let days = $state(30);
  let stats = $state<AdminStats | null>(null);
  let error = $state('');
  let seq = 0;

  async function load() {
    const mine = ++seq;
    error = '';
    const res = await api(`/api/admin/stats?days=${days}`).catch(() => null);
    if (mine !== seq) return;
    if (res?.ok) stats = (await res.json()) as AdminStats;
    else error = 'Could not load statistics.';
  }

  $effect(() => {
    void days;
    void version;
    untrack(load);
  });

  /** Every UTC day of the range, oldest first, as `YYYY-MM-DD`. */
  const labels = $derived.by(() => {
    if (!stats) return [];
    const start = new Date(`${stats.since}T00:00:00Z`).getTime();
    return Array.from({ length: stats.days }, (_, i) => new Date(start + i * 86_400_000).toISOString().slice(0, 10));
  });

  function perDay(rows: { day: string; count: number }[]): number[] {
    const by = new Map<string, number>();
    for (const r of rows) by.set(r.day, (by.get(r.day) ?? 0) + r.count);
    return labels.map((d) => by.get(d) ?? 0);
  }

  const ingest = $derived.by((): Series[] => {
    const rows = stats?.ingest_daily ?? [];
    const pick = (kind: string | null, outcome: string) =>
      perDay(rows.filter((r) => r.outcome === outcome && (kind === null || r.kind === kind)));
    return [
      { label: 'Carrier events stored', fill: 'full', values: pick('carrier', 'stored') },
      { label: 'Markets stored', fill: 'mid', values: pick('market', 'stored') },
      { label: 'Ignored', fill: 'low', values: pick(null, 'ignored') },
      { label: 'Refused', fill: 'text', values: pick(null, 'refused') }
    ];
  });

  const signups = $derived<Series[]>([{ label: 'New accounts', fill: 'full', values: perDay(stats?.accounts.signups_daily ?? []) }]);

  const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

  function formatBytes(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (bytes >= 1024 && i < units.length - 1) {
      bytes /= 1024;
      i++;
    }
    return `${formatNumber(i === 0 ? bytes : Math.round(bytes * 10) / 10)} ${units[i]}`;
  }
</script>

<section class="section">
  <div class="history__head">
    <h2>Traffic</h2>
    <select bind:value={days} aria-label="Range">
      {#each RANGES as r (r)}
        <option value={r}>Last {r} days</option>
      {/each}
    </select>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {:else if !stats}
    <p class="muted">Loading…</p>
  {:else}
    <div class="stats">
      <div class="stat">
        <span class="stat__label">Events stored</span>
        <span class="stat__value">{formatNumber(sum(ingest[0].values))}</span>
      </div>
      <div class="stat">
        <span class="stat__label">Markets stored</span>
        <span class="stat__value">{formatNumber(sum(ingest[1].values))}</span>
      </div>
      <div class="stat">
        <span class="stat__label">Refused</span>
        <span class="stat__value">{formatNumber(sum(ingest[3].values))}</span>
        <span class="stat__hint">another's carrier, a callsign clash or a bad event</span>
      </div>
      <div class="stat">
        <span class="stat__label">Reporting accounts</span>
        <span class="stat__value">{formatNumber(stats.accounts.reporting)}</span>
        <span class="stat__hint">{formatNumber(stats.accounts.signed_in)} signed in</span>
      </div>
    </div>

    <h3 class="chart__title">Plugin uploads per day (UTC)</h3>
    <BarChart {labels} series={ingest} />
    <p class="option__hint">Counted from this release on; earlier traffic wasn't recorded.</p>

    <h3 class="chart__title">New accounts per day</h3>
    <BarChart {labels} series={signups} height={60} />
  {/if}
</section>

{#if stats}
  <section class="section">
    <div class="history__head"><h2>Top reporters</h2></div>
    {#if stats.reporters.length === 0}
      <p class="muted">No uploads in this range.</p>
    {:else}
      <div class="table-wrap">
        <table class="admin-stats">
          <thead><tr><th>Account</th><th>Stored</th><th>Ignored</th><th>Refused</th><th>Last upload</th></tr></thead>
          <tbody>
            {#each stats.reporters as r (r.user_id)}
              <tr>
                <td>
                  {#if r.user_id === 0}
                    <span class="muted">legacy token / anonymous</span>
                  {:else if r.username}
                    <button class="link" onclick={() => onopenuser(r.user_id)}>{r.username}</button>
                  {:else}
                    <span class="muted">deleted account #{r.user_id}</span>
                  {/if}
                </td>
                <td>{formatNumber(r.stored ?? 0)}</td>
                <td>{formatNumber(r.ignored ?? 0)}</td>
                <td>{formatNumber(r.refused ?? 0)}</td>
                <td>{r.last_day}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>

  <section class="section">
    <div class="history__head"><h2>Biggest carriers</h2></div>
    {#if stats.carriers.length === 0}
      <p class="muted">No event history yet.</p>
    {:else}
      <div class="table-wrap">
        <table class="admin-stats">
          <thead><tr><th>Carrier</th><th>Events</th><th>In range</th><th>History size</th><th>Last received</th></tr></thead>
          <tbody>
            {#each stats.carriers as c (c.carrier_id)}
              <tr>
                <td>
                  <button class="link" onclick={() => onopencarrier(c.carrier_id)}>{c.callsign ?? `#${c.carrier_id}`}</button>
                  {#if c.name}<span class="muted">{c.name}</span>{/if}
                </td>
                <td>{formatNumber(c.events)}</td>
                <td>{formatNumber(c.recent)}</td>
                <td>{formatBytes(c.bytes)}</td>
                <td>{c.last_received ? formatTimestamp(c.last_received) : '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>

  <section class="section">
    <div class="history__head">
      <h2>Storage</h2>
      <span class="status status--online">{formatBytes(stats.database_bytes)}</span>
    </div>
    <p class="option__hint">
      {formatNumber(stats.accounts.total)} accounts ({formatNumber(stats.accounts.admins)} admin,
      {formatNumber(stats.accounts.disabled)} disabled, {formatNumber(stats.accounts.live_sessions)} live sessions) ·
      {formatNumber(stats.carrier_counts.total)} carriers ({formatNumber(stats.carrier_counts.owned)} owned,
      {formatNumber(stats.carrier_counts.hidden)} hidden, {formatNumber(stats.carrier_counts.with_market)} with a market)
    </p>
    <div class="table-wrap">
      <table class="admin-stats">
        <thead><tr><th>Table</th><th>Rows (approx.)</th><th>Size</th></tr></thead>
        <tbody>
          {#each stats.tables as t (t.name)}
            <tr><td>{t.name}</td><td>{formatNumber(t.rows)}</td><td>{formatBytes(t.bytes)}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
{/if}
