<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/auth.svelte';
  import type { AdminSettings } from '$lib/types/admin';

  /** Runtime settings. Each change saves at once and is written to the audit log. */

  type Registration = 'open' | 'closed' | 'default';

  let settings = $state<AdminSettings | null>(null);
  let error = $state('');
  let busy = $state(false);

  // The radio group. Kept apart from `settings` so a failed save can put it
  // back to what the server has.
  let selected = $state<Registration | null>(null);

  function stored(s: AdminSettings): Registration {
    const v = s.registration.setting;
    return v === null ? 'default' : v ? 'open' : 'closed';
  }

  async function load() {
    const res = await api('/api/admin/settings').catch(() => null);
    if (res?.ok) {
      settings = (await res.json()) as AdminSettings;
      selected = stored(settings);
    } else error = 'Could not load settings.';
  }

  async function save() {
    if (!settings || !selected || selected === stored(settings)) return;
    const registration = selected;
    busy = true;
    error = '';
    const res = await api('/api/admin/settings', {
      method: 'PUT',
      body: JSON.stringify({ registration })
    }).catch(() => null);
    busy = false;
    if (res?.ok) {
      settings = (await res.json()) as AdminSettings;
    } else {
      error = `Could not save (${res?.status ?? 'network error'}).`;
      selected = stored(settings);
    }
  }

  onMount(load);
</script>

<section class="section">
  <div class="history__head">
    <h2>Settings</h2>
  </div>

  {#if !settings}
    {#if error}<p class="error">{error}</p>{:else}<p class="muted">Loading…</p>{/if}
  {:else}
    <fieldset class="modal__group" disabled={busy}>
      <legend>Registration</legend>
      <p class="option__hint">
        {settings.registration.open ? 'Open: new accounts can be created.' : 'Closed: nobody new can sign up.'}
      </p>
      <label class="option">
        <input type="radio" name="registration" value="open" bind:group={selected} onchange={save} />
        <span>
          <span class="option__label">Open</span>
          <span class="option__hint">Anyone can register.</span>
        </span>
      </label>
      <label class="option">
        <input type="radio" name="registration" value="closed" bind:group={selected} onchange={save} />
        <span>
          <span class="option__label">Closed</span>
          <span class="option__hint">Existing accounts keep working; nobody new can sign up.</span>
        </span>
      </label>
      <label class="option">
        <input type="radio" name="registration" value="default" bind:group={selected} onchange={save} />
        <span>
          <span class="option__label">Server default ({settings.registration.env ? 'open' : 'closed'})</span>
          <span class="option__hint">Follow <code>ALLOW_REGISTRATION</code> in the server's environment.</span>
        </span>
      </label>
    </fieldset>
    {#if error}<p class="error">{error}</p>{/if}
  {/if}
</section>
