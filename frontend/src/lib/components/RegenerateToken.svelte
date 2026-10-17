<script lang="ts">
  import { api } from '$lib/auth.svelte';

  /**
   * Password prompt for giving a token a new secret. Secrets are stored only
   * as hashes, so a lost one can't be shown again; a new one replaces it and
   * the old one stops working. `ondone` receives the new secret.
   */
  let {
    token,
    ondone,
    onclose
  }: {
    token: { id: number; name: string };
    ondone: (secret: string) => void;
    onclose: () => void;
  } = $props();

  let dialog: HTMLDialogElement;
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  $effect(() => {
    dialog.showModal();
  });

  async function submit(e: Event) {
    e.preventDefault();
    busy = true;
    error = '';
    const res = await api(`/api/auth/tokens/${token.id}/regenerate`, {
      method: 'POST',
      body: JSON.stringify({ password })
    });
    busy = false;
    password = '';
    if (!res.ok) {
      error =
        res.status === 403
          ? 'Wrong password.'
          : res.status === 429
            ? 'Too many attempts, wait a moment.'
            : `Could not regenerate (${res.status}).`;
      return;
    }
    ondone(((await res.json()) as { token: string }).token);
    dialog.close();
  }

  function backdropClose(e: MouseEvent) {
    if (e.target === dialog) dialog.close();
  }
</script>

<dialog class="modal" bind:this={dialog} onclose={onclose} onclick={backdropClose}>
  <form class="modal__panel modal__form" onsubmit={submit}>
    <header class="modal__head">
      <h3>Regenerate token</h3>
      <span class="modal__sub">{token.name}</span>
    </header>

    <p class="option__hint">
      The current secret stops working at once; paste the new one into the EDMC plugin. Enter
      your password to continue.
    </p>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      type="password"
      autocomplete="current-password"
      placeholder="password"
      bind:value={password}
      disabled={busy}
      required
      autofocus
    />

    {#if error}<p class="error">{error}</p>{/if}

    <div class="modal__foot">
      <button type="submit" class="button--danger" disabled={busy || !password}>Regenerate</button>
      <button type="button" onclick={() => dialog.close()}>Cancel</button>
    </div>
  </form>
</dialog>
