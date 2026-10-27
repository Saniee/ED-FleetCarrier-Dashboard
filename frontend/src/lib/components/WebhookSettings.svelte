<script lang="ts">
  import { api } from '$lib/auth.svelte';

  /**
   * The carrier's Discord webhook, inside the owner's settings dialog. Unlike the
   * rest of the dialog it saves on a button: the URL is pasted whole, and a
   * half-typed one would be refused.
   */
  let { callsign }: { callsign: string } = $props();

  type WebhookEvent =
    | 'jump'
    | 'arrival'
    | 'trade_order'
    | 'order_filled'
    | 'low_fuel'
    | 'low_balance'
    | 'operation';

  interface Webhook {
    configured: boolean;
    url?: string;
    events: WebhookEvent[];
    low_fuel_threshold: number;
    low_balance_weeks: number;
    enabled?: boolean;
    last_sent_at?: string | null;
    last_error?: string | null;
  }

  const EVENTS: { key: WebhookEvent; label: string }[] = [
    { key: 'jump', label: 'Jump scheduled or cancelled' },
    { key: 'arrival', label: 'Arrived in a new system' },
    { key: 'trade_order', label: 'Trade order posted or cancelled' },
    { key: 'order_filled', label: 'Trade order filled' },
    { key: 'low_fuel', label: 'Tritium running low' },
    { key: 'low_balance', label: 'Balance running low' },
    { key: 'operation', label: 'Hauling operation complete' }
  ];

  const path = $derived(`/api/carriers/${encodeURIComponent(callsign)}/webhook`);

  let hook = $state<Webhook | null>(null);
  let url = $state('');
  let events = $state<WebhookEvent[]>([]);
  let threshold = $state(200);
  let balanceWeeks = $state(4);
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');

  async function load() {
    const res = await api(path);
    if (!res.ok) {
      error = `Could not load the webhook (${res.status}).`;
      return;
    }
    hook = await res.json();
    events = [...hook!.events];
    threshold = hook!.low_fuel_threshold;
    balanceWeeks = hook!.low_balance_weeks;
    url = '';
  }

  $effect(() => {
    load();
  });

  function toggle(key: WebhookEvent, on: boolean) {
    events = on ? [...events, key] : events.filter((e) => e !== key);
  }

  async function run(method: string, suffix = '', body?: unknown, done = '') {
    busy = true;
    error = '';
    notice = '';
    const res = await api(path + suffix, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    busy = false;
    if (!res.ok) {
      error = (await res.text().catch(() => '')) || `Failed (${res.status}).`;
    } else {
      notice = done;
    }
    await load();
  }

  const save = () =>
    run('PUT', '', {
      url: url.trim() || undefined,
      events,
      low_fuel_threshold: threshold,
      low_balance_weeks: balanceWeeks
    }, 'Saved.');
  const test = () => run('POST', '/test', undefined, 'Test message sent.');
  const remove = () => run('DELETE', '', undefined, 'Webhook removed.');
</script>

<fieldset class="modal__group" disabled={busy || !hook}>
  <legend>Discord</legend>
  <p class="option__hint">
    Post this carrier's jumps, trade orders, low tritium and low balance to a Discord channel. In Discord:
    channel settings → Integrations → Webhooks → New Webhook → Copy Webhook URL.
  </p>

  {#if hook}
    <input
      type="url"
      bind:value={url}
      placeholder={hook.configured ? hook.url : 'https://discord.com/api/webhooks/…'}
      autocomplete="off"
    />

    {#each EVENTS as e}
      <label class="option">
        <input
          type="checkbox"
          checked={events.includes(e.key)}
          onchange={(ev) => toggle(e.key, ev.currentTarget.checked)}
        />
        <span class="option__label">{e.label}</span>
      </label>
    {/each}

    {#if events.includes('low_fuel')}
      <label class="option">
        <input type="number" min="0" max="1000" bind:value={threshold} />
        <span class="option__hint">Alert below this many tonnes of tritium.</span>
      </label>
    {/if}

    {#if events.includes('low_balance')}
      <label class="option">
        <input type="number" min="0" max="52" bind:value={balanceWeeks} />
        <span class="option__hint">
          Alert below this many weeks of upkeep. Needs the weekly upkeep, set on the carrier's
          Finances page.
        </span>
      </label>
    {/if}

    {#if hook.configured}
      {#if hook.enabled === false}
        <p class="option__hint">Switched off: {hook.last_error}. Save a new URL to turn it back on.</p>
      {:else if hook.last_error}
        <p class="option__hint">Last error: {hook.last_error}</p>
      {:else if hook.last_sent_at}
        <p class="option__hint">Last sent {new Date(hook.last_sent_at).toLocaleString()}.</p>
      {/if}
    {/if}

    <div class="form__actions">
      <button onclick={save} disabled={!hook.configured && !url.trim()}>Save</button>
      {#if hook.configured}
        <button onclick={test}>Send test</button>
        <button onclick={remove}>Remove</button>
      {/if}
    </div>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
  {#if notice}<p class="option__hint">{notice}</p>{/if}
</fieldset>
