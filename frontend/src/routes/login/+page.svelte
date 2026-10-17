<script lang="ts">
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';

  let username = $state('');
  let password = $state('');
  let error = $state('');
  let busy = $state(false);

  async function submit(action: 'login' | 'register') {
    busy = true;
    error = await auth[action](username, password);
    busy = false;
    if (!error) await goto('/account');
  }
</script>

<svelte:head>
  <title>Login · ED Commander</title>
</svelte:head>

<div class="carrier-page">
  <div class="history__head">
    <h2>Login</h2>
  </div>

  <form class="form--stack" onsubmit={(e) => { e.preventDefault(); void submit('login'); }}>
    <input bind:value={username} placeholder="username" autocomplete="username" />
    <input bind:value={password} type="password" placeholder="password" autocomplete="current-password" />
    <div class="form__actions">
      <button type="submit" disabled={busy}>Login</button>
      <button type="button" disabled={busy} onclick={() => submit('register')}>Register</button>
    </div>
  </form>

  <p class="muted">Usernames are 3-32 characters (letters, digits, _ or -). Passwords are 8-128.</p>
  {#if error}<p class="error">{error}</p>{/if}
</div>
