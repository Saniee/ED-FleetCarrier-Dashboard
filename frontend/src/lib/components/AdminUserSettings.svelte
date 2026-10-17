<script lang="ts">
  import { api, auth } from '$lib/auth.svelte';
  import { formatTimestamp } from '$lib/format';
  import type { AdminUserDetail } from '$lib/types/admin';
  import { VISIBILITY_LABEL } from '$lib/visibility';

  /**
   * Moderation for one account, as a modal. Each action saves at once and is
   * written to the audit log; `onchange` lets the list re-read, `onclose`
   * fires however the dialog is dismissed.
   */
  let {
    userId,
    onchange,
    onhistory,
    onclose
  }: {
    userId: number;
    onchange: () => void;
    /** Show the audit log for this account. */
    onhistory: (user: { id: number; name: string }) => void;
    onclose: () => void;
  } = $props();

  let dialog: HTMLDialogElement;
  let user = $state<AdminUserDetail | null>(null);
  let error = $state('');
  let busy = $state(false);
  let reason = $state('');
  let confirmName = $state('');
  let confirmingDelete = $state(false);

  const isSelf = $derived(user?.id === auth.user?.id);
  const base = $derived(`/api/admin/users/${userId}`);

  async function load() {
    const res = await api(base);
    if (res.ok) user = (await res.json()) as AdminUserDetail;
    else error = res.status === 404 ? 'This account no longer exists.' : `Could not load (${res.status}).`;
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

  async function disable() {
    if (await act('POST', '/disable', { reason: reason.trim() || null })) {
      reason = '';
      await load();
    }
  }

  async function run(method: string, suffix: string, body?: unknown) {
    if (await act(method, suffix, body)) await load();
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
    {#if !user}
      <header class="modal__head"><h3>Account</h3></header>
      {#if error}<p class="error">{error}</p>{:else}<p class="muted">Loading…</p>{/if}
    {:else}
      <header class="modal__head">
        <h3>{user.username}</h3>
        <span class="modal__sub">#{user.id} · joined {formatTimestamp(user.created_at)}</span>
        {#if user.role === 'admin'}<span class="tag">Admin</span>{/if}
        {#if user.disabled_at}<span class="tag tag--alert">Disabled</span>{/if}
      </header>

      {#if isSelf}
        <p class="option__hint modal__note">This is your account: you can't disable, demote or delete it here.</p>
      {/if}

      <fieldset class="modal__group" disabled={busy}>
        <legend>Status</legend>
        {#if user.disabled_at}
          <p class="option__hint">
            Disabled {formatTimestamp(user.disabled_at)}{user.disabled_reason ? `: ${user.disabled_reason}` : ''}.
            Their carriers are hidden from everyone but admins.
          </p>
          <div class="form__actions">
            <button onclick={() => run('POST', '/enable')}>Enable</button>
          </div>
        {:else}
          <p class="option__hint">
            Disabling signs them out, stops their plugin tokens and hides their carriers until enabled.
          </p>
          <div class="modal__row">
            <input bind:value={reason} maxlength="500" placeholder="reason (optional)" disabled={isSelf} />
            <button onclick={disable} disabled={isSelf}>Disable</button>
          </div>
        {/if}
      </fieldset>

      <fieldset class="modal__group" disabled={busy}>
        <legend>Role</legend>
        <p class="option__hint">
          {user.role === 'admin' ? 'Can open this dashboard and moderate accounts.' : 'A regular account.'}
        </p>
        <div class="form__actions">
          {#if user.role === 'admin'}
            <button onclick={() => run('PATCH', '', { role: 'user' })} disabled={isSelf}>Remove admin</button>
          {:else}
            <button onclick={() => run('PATCH', '', { role: 'admin' })}>Make admin</button>
          {/if}
        </div>
      </fieldset>

      <fieldset class="modal__group" disabled={busy}>
        <legend>Access</legend>
        <div class="modal__row">
          <span class="option__hint">{user.active_sessions} live session{user.active_sessions === 1 ? '' : 's'}</span>
          <button onclick={() => run('DELETE', '/sessions')} disabled={user.active_sessions === 0}>
            Sign out everywhere
          </button>
        </div>
        {#if user.token_list.length === 0}
          <p class="option__hint">No plugin tokens.</p>
        {:else}
          <table class="modal__table">
            <thead><tr><th>Token</th><th>Created</th><th>Last used</th><th></th></tr></thead>
            <tbody>
              {#each user.token_list as t (t.id)}
                <tr>
                  <td>{t.name}</td>
                  <td>{formatTimestamp(t.created_at)}</td>
                  <td>{t.last_used_at ? formatTimestamp(t.last_used_at) : 'never'}</td>
                  <td class="actions"><button onclick={() => run('DELETE', `/tokens/${t.id}`)}>Revoke</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </fieldset>

      <fieldset class="modal__group">
        <legend>Carriers</legend>
        {#if user.carrier_list.length === 0}
          <p class="option__hint">None.</p>
        {:else}
          <table class="modal__table">
            <thead><tr><th>Callsign</th><th>Name</th><th>Visibility</th></tr></thead>
            <tbody>
              {#each user.carrier_list as c (c.carrier_id)}
                <tr>
                  <td>
                    {#if c.callsign}<a href={`/carrier/${encodeURIComponent(c.callsign)}`}>{c.callsign}</a>{:else}—{/if}
                  </td>
                  <td>{c.name ?? '—'}</td>
                  <td>{VISIBILITY_LABEL[c.visibility]}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </fieldset>

      <fieldset class="modal__group modal__group--danger" disabled={busy || isSelf}>
        <legend>Delete account</legend>
        <p class="option__hint">
          Removes the account, its sessions and tokens. Its carriers keep their history but become
          unowned, so the real owner's plugin can claim them again.
        </p>
        {#if confirmingDelete}
          <div class="modal__row">
            <input bind:value={confirmName} placeholder={`type ${user.username} to confirm`} autocomplete="off" />
            <button class="button--danger" onclick={remove} disabled={confirmName !== user.username}>Delete</button>
            <button onclick={() => ((confirmingDelete = false), (confirmName = ''))}>Keep</button>
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
      {#if user}
        {@const u = user}
        <button onclick={() => onhistory({ id: u.id, name: u.username })}>History</button>
      {/if}
      <button onclick={() => dialog.close()}>Close</button>
    </div>
  </div>
</dialog>
