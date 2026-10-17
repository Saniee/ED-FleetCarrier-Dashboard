<script lang="ts">
  import { onMount } from 'svelte';
  import { goto, replaceState } from '$app/navigation';
  import { page as route } from '$app/state';
  import { auth, api } from '$lib/auth.svelte';
  import { formatTimestamp } from '$lib/format';
  import type { AdminUserPage } from '$lib/types/admin';
  import AdminUserSettings from '$lib/components/AdminUserSettings.svelte';
  import AdminSettings from '$lib/components/AdminSettings.svelte';
  import AdminAudit from '$lib/components/AdminAudit.svelte';

  const PER_PAGE = 25;
  const TABS = { accounts: 'Accounts', settings: 'Settings', audit: 'Audit log' } as const;
  type Tab = keyof typeof TABS;

  const initialTab = route.url.searchParams.get('tab');
  let tab = $state<Tab>(initialTab && initialTab in TABS ? (initialTab as Tab) : 'accounts');
  // The audit log's account filter; set from an account's "History".
  let auditUser = $state<{ id: number; name: string } | null>(null);
  // Bumped on every change from the account dialog, so an open log re-reads.
  let auditVersion = $state(0);

  let result = $state<AdminUserPage | null>(null);
  let page = $state(1);
  let query = $state('');
  let error = $state('');
  let denied = $state(false);
  // Account whose dialog is open. `?user=` opens one on arrival.
  let managing = $state<number | null>(Number(route.url.searchParams.get('user')) || null);

  // Guards against a slow response landing after a newer one.
  let seq = 0;

  async function load() {
    const mine = ++seq;
    error = '';
    const params = new URLSearchParams({ page: String(page), per_page: String(PER_PAGE) });
    if (query.trim()) params.set('q', query.trim());
    const res = await api(`/api/admin/users?${params}`).catch(() => null);
    if (mine !== seq) return;
    if (res?.status === 401) {
      await auth.logout();
      await goto('/login');
    } else if (res?.status === 403) {
      denied = true;
    } else if (res?.ok) {
      result = (await res.json()) as AdminUserPage;
    } else {
      error = 'Could not load accounts.';
    }
  }

  function search(e: Event) {
    e.preventDefault();
    page = 1;
    void load();
  }

  function show(next: Tab) {
    tab = next;
    const url = new URL(route.url);
    if (next === 'accounts') url.searchParams.delete('tab');
    else url.searchParams.set('tab', next);
    url.searchParams.delete('user');
    replaceState(url, {});
  }

  function changed() {
    auditVersion++;
    void load();
  }

  function history(user: { id: number; name: string }) {
    auditUser = user;
    managing = null;
    show('audit');
  }

  function go(next: number) {
    page = next;
    void load();
  }

  onMount(() => {
    if (!auth.user) void goto('/login');
    else void load();
  });
</script>

<svelte:head>
  <title>Admin · ED Commander</title>
</svelte:head>

<div class="carrier-page">
  {#if denied}
    <p class="error">Admins only.</p>
  {:else if auth.user}
    <div class="tabs" role="tablist">
      {#each Object.entries(TABS) as [key, label] (key)}
        <button role="tab" aria-selected={tab === key} onclick={() => show(key as Tab)}>{label}</button>
      {/each}
    </div>

    {#if tab === 'settings'}
      <AdminSettings />
    {:else if tab === 'audit'}
      <AdminAudit user={auditUser} version={auditVersion} onfilter={(u) => (auditUser = u)} onopen={(id) => (managing = id)} />
    {:else}
      <section class="section">
        <div class="history__head">
          <h2>Accounts</h2>
          {#if result}<span class="status status--online">{result.total} total</span>{/if}
        </div>

        <form onsubmit={search}>
          <input bind:value={query} placeholder="username" />
          <button type="submit">Search</button>
        </form>

        {#if error}
          <p class="error">{error}</p>
        {:else if !result}
          <p class="muted">Loading…</p>
        {:else if result.items.length === 0}
          <p class="muted">No accounts found.</p>
        {:else}
          <div class="table-wrap">
            <table class="admin-users">
              <thead>
                <tr>
                  <th>User</th>
                  <th>Carriers</th>
                  <th>Tokens</th>
                  <th>Last seen</th>
                  <th>Joined</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {#each result.items as u (u.id)}
                  <tr>
                    <td>
                      {u.username}
                      {#if u.role === 'admin'}<span class="tag">Admin</span>{/if}
                      {#if u.disabled_at}<span class="tag tag--alert">Disabled</span>{/if}
                      {#if u.id === auth.user.id}<span class="tag">You</span>{/if}
                    </td>
                    <td>{u.carriers}</td>
                    <td>{u.tokens}</td>
                    <td>{u.last_seen ? formatTimestamp(u.last_seen) : 'never'}</td>
                    <td>{formatTimestamp(u.created_at)}</td>
                    <td class="actions"><button onclick={() => (managing = u.id)}>Manage</button></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>

          <p class="pager">
            <button disabled={result.page <= 1} onclick={() => go(result!.page - 1)}>Prev</button>
            <span>Page {result.page} of {Math.max(1, result.total_pages)}</span>
            <button disabled={result.page >= result.total_pages} onclick={() => go(result!.page + 1)}>Next</button>
          </p>
        {/if}
      </section>
    {/if}

    {#if managing !== null}
      <AdminUserSettings userId={managing} onchange={changed} onhistory={history} onclose={() => (managing = null)} />
    {/if}
  {/if}
</div>
