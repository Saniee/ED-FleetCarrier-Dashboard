<script lang="ts">
  import { api } from '$lib/auth.svelte';
  import type { CarrierListItem } from '$lib/types/carrier';
  import { isSquadronHull } from '$lib/carrier-fuel';
  import { VISIBILITIES, VISIBILITY_LABEL, type Visibility } from '$lib/visibility';

  /**
   * Owner settings for one carrier, as a modal. Changes save as they are made;
   * `onchange` lets the page re-read its list, `onclose` fires however the
   * dialog is dismissed (button, Esc or backdrop).
   */
  let {
    carrier,
    onchange,
    onclose
  }: { carrier: CarrierListItem; onchange: () => Promise<void>; onclose: () => void } = $props();

  const VISIBILITY_HINT: Record<Visibility, string> = {
    public: 'Listed on the carriers page; anyone can open it.',
    private: 'Not listed; anyone with the link can open it.',
    owner_only: 'Only you can open it.'
  };

  let dialog: HTMLDialogElement;
  let error = $state('');
  let busy = $state(false);
  let confirmingRelease = $state(false);

  const path = $derived(`/api/carriers/${encodeURIComponent(carrier.callsign ?? '')}`);
  const squadronHull = $derived(isSquadronHull(carrier.carrier_type));

  $effect(() => {
    dialog.showModal();
  });

  async function send(method: string, suffix: string, body?: unknown): Promise<boolean> {
    busy = true;
    error = '';
    const res = await api(path + suffix, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    busy = false;
    if (!res.ok) {
      error = `Could not save (${res.status}).`;
      return false;
    }
    await onchange();
    return true;
  }

  async function release() {
    if (await send('DELETE', '/claim')) dialog.close();
  }

  // Clicks on the dialog element itself land on the backdrop, outside the panel.
  function backdropClose(e: MouseEvent) {
    if (e.target === dialog) dialog.close();
  }
</script>

<dialog class="modal" bind:this={dialog} onclose={onclose} onclick={backdropClose}>
  <!-- showModal() would focus the first radio and ring it as if tabbed to;
       starting on the panel leaves Tab to step into the controls. -->
  <!-- svelte-ignore a11y_autofocus -->
  <div class="modal__panel" tabindex="-1" autofocus>
    <header class="modal__head">
      <h3>{carrier.name ?? 'Unknown carrier'}</h3>
      <span class="modal__sub">{carrier.callsign}</span>
    </header>

    <fieldset class="modal__group" disabled={busy}>
      <legend>Visibility</legend>
      {#each VISIBILITIES as v}
        <label class="option">
          <input
            type="radio"
            name="visibility"
            value={v}
            checked={carrier.visibility === v}
            onchange={() => send('PATCH', '', { visibility: v })}
          />
          <span>
            <span class="option__label">{VISIBILITY_LABEL[v]}</span>
            <span class="option__hint">{VISIBILITY_HINT[v]}</span>
          </span>
        </label>
      {/each}
    </fieldset>

    <fieldset class="modal__group" disabled={busy}>
      <legend>Squadron</legend>
      <label class="option">
        <input
          type="checkbox"
          checked={carrier.is_squadron || squadronHull}
          disabled={squadronHull}
          onchange={(e) => send('PATCH', '', { is_squadron: e.currentTarget.checked })}
        />
        <span>
          <span class="option__label">Squadron carrier</span>
          <span class="option__hint">
            {squadronHull
              ? 'The game reports this as a squadron carrier.'
              : "Mark this carrier as your squadron's."}
          </span>
        </span>
      </label>
    </fieldset>

    <fieldset class="modal__group modal__group--danger" disabled={busy}>
      <legend>Ownership</legend>
      <p class="option__hint">
        Releasing leaves the carrier unowned. Your plugin claims it again the next time it sends
        CarrierStats.
      </p>
      {#if confirmingRelease}
        <div class="form__actions">
          <button class="button--danger" onclick={release}>Confirm release</button>
          <button onclick={() => (confirmingRelease = false)}>Keep</button>
        </div>
      {:else}
        <div class="form__actions">
          <button onclick={() => (confirmingRelease = true)}>Release</button>
        </div>
      {/if}
    </fieldset>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="modal__foot">
      <button onclick={() => dialog.close()}>Close</button>
    </div>
  </div>
</dialog>
