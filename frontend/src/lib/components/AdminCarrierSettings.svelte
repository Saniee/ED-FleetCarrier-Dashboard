<script lang="ts">
  import { api } from '$lib/auth.svelte';
  import { formatNumber, formatTimestamp } from '$lib/format';
  import type { AdminCarrierDetail, AuditFilter } from '$lib/types/admin';
  import { VISIBILITY_LABEL } from '$lib/visibility';

  /**
   * Moderation for one carrier, as a modal. Each action saves at once and is
   * written to the audit log; `onchange` lets the lists re-read, `onclose`
   * fires however the dialog is dismissed.
   */
  let {
    carrierId,
    onchange,
    onhistory,
    onopenuser,
    onclose
  }: {
    carrierId: number;
    onchange: () => void;
    /** Show the audit log for this carrier. */
    onhistory: (filter: AuditFilter) => void;
    onopenuser: (id: number) => void;
    onclose: () => void;
  } = $props();

  let dialog: HTMLDialogElement;
  let carrier = $state<AdminCarrierDetail | null>(null);
  let error = $state('');
  let busy = $state(false);
  let reason = $state('');
  let newOwner = $state('');
  let confirmCallsign = $state('');
  let confirmingDelete = $state(false);
  // Trim: drop events from before this day (YYYY-MM-DD, UTC).
  let trimBefore = $state('');
  let trimmed = $state<number | null>(null);

  const base = $derived(`/api/admin/carriers/${carrierId}`);
  // What has to be typed to delete: the callsign, or the id without one.
  const label = $derived(carrier ? (carrier.callsign ?? String(carrier.carrier_id)) : '');

  async function load() {
    const res = await api(base);
    if (res.ok) carrier = (await res.json()) as AdminCarrierDetail;
    else error = res.status === 404 ? 'This carrier no longer exists.' : `Could not load (${res.status}).`;
  }

  $effect(() => {
    dialog.showModal();
    void load();
  });

  /** Run one action; the server's message is shown when it refuses. */
  async function act(method: string, suffix: string, body?: unknown): Promise<boolean> {
    busy = true;
    error = '';
    const res = await api(base + suffix, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    busy = false;
    if (!res.ok) {
      const msg = await res.text().catch(() => '');
      error = msg ? `${msg.charAt(0).toUpperCase()}${msg.slice(1)}.` : `Could not save (${res.status}).`;
      return false;
    }
    onchange();
    return true;
  }

  async function run(method: string, suffix: string, body?: unknown) {
    if (await act(method, suffix, body)) await load();
  }

  async function hide() {
    if (await act('POST', '/hide', { reason: reason.trim() || null })) {
      reason = '';
      await load();
    }
  }

  async function reassign(e: Event) {
    e.preventDefault();
    if (await act('PUT', '/owner', { username: newOwner.trim() })) {
      newOwner = '';
      await load();
    }
  }

  async function trim(e: Event) {
    e.preventDefault();
    trimmed = null;
    busy = true;
    error = '';
    const res = await api(`${base}/events?before=${encodeURIComponent(trimBefore)}`, { method: 'DELETE' });
    busy = false;
    if (!res.ok) {
      error = `Could not trim (${res.status}).`;
      return;
    }
    trimmed = ((await res.json()) as { deleted: number }).deleted;
    onchange();
    await load();
  }

  async function remove() {
    if (await act('DELETE', '')) dialog.close();
  }

  function backdropClose(e: MouseEvent) {
    if (e.target === dialog) dialog.close();
  }
</script>

<dialog class="modal modal--wide" bind:this={dialog} onclose={onclose} onclick={backdropClose}>
  <!-- svelte-ignore a11y_autofocus -->
  <div class="modal__panel" tabindex="-1" autofocus>
    {#if !carrier}
      <header class="modal__head"><h3>Carrier</h3></header>
      {#if error}<p class="error">{error}</p>{:else}<p class="muted">Loading…</p>{/if}
    {:else}
      <header class="modal__head">
        <h3>{carrier.name ?? 'unknown carrier'}</h3>
        <span class="modal__sub">{carrier.callsign ?? '—'} · #{carrier.carrier_id}</span>
        {#if carrier.moderation_hidden_at}<span class="tag tag--alert">Hidden</span>{/if}
      </header>

      <p class="option__hint modal__note">
        {VISIBILITY_LABEL[carrier.visibility]} ·
        {formatNumber(carrier.events)} events · {formatNumber(carrier.commodities)} market lines
        {#if carrier.updated_at}· updated {formatTimestamp(carrier.updated_at)}{/if}
        {#if carrier.callsign && carrier.visibility !== 'owner_only'}
          · <a href={`/carrier/${encodeURIComponent(carrier.callsign)}`}>open page</a>
        {/if}
      </p>

      <fieldset class="modal__group" disabled={busy}>
        <legend>Visibility</legend>
        {#if carrier.moderation_hidden_at}
          <p class="option__hint">
            Hidden {formatTimestamp(carrier.moderation_hidden_at)}{carrier.moderation_reason ? `: ${carrier.moderation_reason}` : ''}.
            Only its owner, who sees the reason, and admins can open it.
          </p>
          <div class="form__actions">
            <button onclick={() => run('POST', '/unhide')}>Unhide</button>
          </div>
        {:else}
          <p class="option__hint">
            Hiding overrides the owner's setting: it leaves the carrier list and its page answers
            "not found" to everyone but the owner and admins.
          </p>
          <div class="modal__row">
            <input bind:value={reason} maxlength="500" placeholder="reason, shown to the owner (optional)" />
            <button onclick={hide}>Hide</button>
          </div>
        {/if}
      </fieldset>

      <fieldset class="modal__group" disabled={busy}>
        <legend>Owner</legend>
        <div class="modal__row">
          <span class="option__hint">
            {#if carrier.owner_id !== null}
              Owned by <button class="link" onclick={() => onopenuser(carrier!.owner_id!)}>{carrier.owner}</button>{carrier.owner_disabled ? ' (disabled)' : ''}.
            {:else}
              Unowned: the next CarrierStats from a plugin claims it.
            {/if}
          </span>
          {#if carrier.owner_id !== null}
            <button onclick={() => run('PUT', '/owner', { username: null })}>Release</button>
          {/if}
        </div>
        <form class="modal__row" onsubmit={reassign}>
          <input bind:value={newOwner} placeholder="give to username" autocomplete="off" />
          <button type="submit" disabled={!newOwner.trim()}>Reassign</button>
        </form>
      </fieldset>

      <fieldset class="modal__group" disabled={busy}>
        <legend>History</legend>
        <p class="option__hint">
          Delete events from before a day (by their journal time), keeping the carrier and its newer
          history.
        </p>
        <form class="modal__row" onsubmit={trim}>
          <input type="date" bind:value={trimBefore} aria-label="Delete events before" />
          <button type="submit" disabled={!trimBefore}>Trim</button>
          {#if trimmed !== null}
            <span class="option__hint">Deleted {formatNumber(trimmed)} event{trimmed === 1 ? '' : 's'}.</span>
          {/if}
        </form>
      </fieldset>

      <fieldset class="modal__group modal__group--danger" disabled={busy}>
        <legend>Delete carrier</legend>
        <p class="option__hint">
          Removes the carrier with its event history and market. It comes back, empty, with the next
          event anyone reports for it, so this is for clearing bad data, not for banning.
        </p>
        {#if confirmingDelete}
          <div class="modal__row">
            <input bind:value={confirmCallsign} placeholder={`type ${label} to confirm`} autocomplete="off" />
            <button class="button--danger" onclick={remove} disabled={confirmCallsign !== label}>Delete</button>
            <button onclick={() => ((confirmingDelete = false), (confirmCallsign = ''))}>Keep</button>
          </div>
        {:else}
          <div class="form__actions">
            <button onclick={() => (confirmingDelete = true)}>Delete…</button>
          </div>
        {/if}
      </fieldset>

      {#if error}<p class="error">{error}</p>{/if}
    {/if}

    <div class="modal__foot">
      {#if carrier}
        {@const c = carrier}
        <button onclick={() => onhistory({ kind: 'carrier', id: c.carrier_id, name: c.callsign ?? `#${c.carrier_id}` })}>History</button>
      {/if}
      <button onclick={() => dialog.close()}>Close</button>
    </div>
  </div>
</dialog>
