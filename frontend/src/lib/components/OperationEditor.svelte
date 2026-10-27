<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/auth.svelte';
  import type { Depot, KnownCommodity, TargetInput } from '$lib/types/operations';

  /**
   * Start an operation on a carrier, or (with `id`) change one's name and
   * targets. A colonisation operation takes its targets from a construction
   * depot some plugin has reported; otherwise the owner lists them.
   */
  let {
    callsign,
    id = null,
    name: initialName = '',
    targets: initialTargets = [],
    depot: hasDepot = false,
    onsaved,
    onclose
  }: {
    callsign: string;
    id?: number | null;
    name?: string;
    targets?: TargetInput[];
    depot?: boolean;
    onsaved: (id: number) => void;
    onclose: () => void;
  } = $props();

  let dialog: HTMLDialogElement;
  // The form starts from the props once; later prop changes don't reset it.
  const editing = untrack(() => id !== null);

  let name = $state(untrack(() => initialName));
  let mode = $state<'depot' | 'manual'>(untrack(() => (hasDepot ? 'depot' : 'manual')));
  let rows = $state<{ commodity: string; required: number | null }[]>(
    untrack(() =>
      initialTargets.length
        ? initialTargets.map((t) => ({ commodity: t.name ?? t.commodity, required: t.required }))
        : [{ commodity: '', required: null }]
    )
  );
  let search = $state('');
  let depots = $state<Depot[]>([]);
  let depotId = $state<number | null>(null);
  let known = $state<KnownCommodity[]>([]);
  let busy = $state(false);
  let error = $state('');

  $effect(() => {
    dialog.showModal();
    api('/api/commodities').then(async (res) => {
      if (res.ok) known = await res.json();
    });
  });

  // Search as the owner types, after a pause.
  $effect(() => {
    if (mode !== 'depot' || editing) return;
    const q = search;
    const timer = setTimeout(async () => {
      const res = await api(`/api/depots?q=${encodeURIComponent(q)}`);
      if (res.ok) depots = await res.json();
    }, 250);
    return () => clearTimeout(timer);
  });

  /** The key for a typed commodity: a known name maps back to its key. */
  function key(input: string): string {
    const typed = input.trim().toLowerCase();
    return known.find((k) => k.name.toLowerCase() === typed)?.commodity ?? typed.replace(/\s+/g, '');
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    const targets: TargetInput[] = rows
      .filter((r) => r.commodity.trim() && r.required)
      .map((r) => ({ commodity: key(r.commodity), name: r.commodity.trim(), required: Number(r.required) }));

    let res: Response;
    busy = true;
    if (editing) {
      res = await api(`/api/operations/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(hasDepot ? { name } : { name, targets })
      });
    } else {
      if (mode === 'depot' && depotId === null) {
        busy = false;
        error = 'Pick a depot.';
        return;
      }
      res = await api(`/api/carriers/${encodeURIComponent(callsign)}/operations`, {
        method: 'POST',
        body: JSON.stringify(
          mode === 'depot' ? { name, depot_market_id: depotId } : { name, targets }
        )
      });
    }
    busy = false;
    if (!res.ok) {
      error = (await res.text().catch(() => '')) || `Could not save (${res.status}).`;
      return;
    }
    const saved = editing ? id! : ((await res.json()) as { id: number }).id;
    onsaved(saved);
    dialog.close();
  }

  function backdropClose(e: MouseEvent) {
    if (e.target === dialog) dialog.close();
  }
</script>

<dialog class="modal" bind:this={dialog} onclose={onclose} onclick={backdropClose}>
  <!-- svelte-ignore a11y_autofocus -->
  <form class="modal__panel" tabindex="-1" autofocus onsubmit={save}>
    <header class="modal__head">
      <h3>{editing ? 'Edit operation' : 'New operation'}</h3>
      <span class="modal__sub">{callsign}</span>
    </header>

    <fieldset class="modal__group" disabled={busy}>
      <legend>Name</legend>
      <input bind:value={name} maxlength="100" required placeholder="e.g. Col 285 outpost" />
    </fieldset>

    {#if !editing}
      <fieldset class="modal__group" disabled={busy}>
        <legend>Targets</legend>
        <label class="option">
          <input type="radio" bind:group={mode} value="depot" />
          <span>
            <span class="option__label">Colonisation depot</span>
            <span class="option__hint">Track a construction site's needs as the game reports them.</span>
          </span>
        </label>
        <label class="option">
          <input type="radio" bind:group={mode} value="manual" />
          <span>
            <span class="option__label">My own list</span>
            <span class="option__hint">Commodities and amounts to collect on the carrier.</span>
          </span>
        </label>
      </fieldset>
    {/if}

    {#if mode === 'depot' && !editing}
      <fieldset class="modal__group" disabled={busy}>
        <legend>Depot</legend>
        <input bind:value={search} placeholder="Search system or station" />
        {#each depots as d (d.market_id)}
          <label class="option">
            <input type="radio" bind:group={depotId} value={d.market_id} />
            <span>
              <span class="option__label">{d.station_name ?? 'Construction site'}</span>
              <span class="option__hint">
                {d.star_system ?? 'unknown system'} · {Math.round(d.progress * 100)}% built
              </span>
            </span>
          </label>
        {:else}
          <p class="option__hint">
            No depots found. Dock at the construction site with the plugin running, then search again.
          </p>
        {/each}
      </fieldset>
    {:else if mode === 'manual'}
      <fieldset class="modal__group" disabled={busy}>
        <legend>Commodities</legend>
        <datalist id="known-commodities">
          {#each known as k (k.commodity)}<option value={k.name}></option>{/each}
        </datalist>
        {#each rows as row, i (i)}
          <div class="form__actions">
            <input list="known-commodities" bind:value={row.commodity} placeholder="Commodity" />
            <input type="number" min="1" bind:value={row.required} placeholder="Tonnes" />
            <button type="button" onclick={() => rows.splice(i, 1)} disabled={rows.length === 1}>×</button>
          </div>
        {/each}
        <div class="form__actions">
          <button type="button" onclick={() => rows.push({ commodity: '', required: null })}>Add commodity</button>
        </div>
      </fieldset>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="modal__foot">
      <button type="submit" disabled={busy}>{editing ? 'Save' : 'Start'}</button>
      <button type="button" onclick={() => dialog.close()}>Cancel</button>
    </div>
  </form>
</dialog>
