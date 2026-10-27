<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '$lib/auth.svelte';
  import { formatTimestamp } from '$lib/format';
  import type { AuditEntry, AuditFilter, AuditPage } from '$lib/types/admin';

  /**
   * The audit log, newest first. `filter` narrows it to one account (with
   * actions on carriers it owned or was given) or one carrier; `onfilter` asks
   * the page to change that (null shows everything). A new `version` re-reads
   * the current page; `onopenuser` opens an account.
   */
  let {
    filter,
    version,
    onfilter,
    onopenuser
  }: {
    filter: AuditFilter | null;
    version: number;
    onfilter: (filter: AuditFilter | null) => void;
    onopenuser: (id: number) => void;
  } = $props();

  const PER_PAGE = 50;

  let result = $state<AuditPage | null>(null);
  let page = $state(1);
  let error = $state('');
  let seq = 0;

  async function load() {
    const mine = ++seq;
    error = '';
    const params = new URLSearchParams({ page: String(page), per_page: String(PER_PAGE) });
    if (filter) params.set(filter.kind, String(filter.id));
    const res = await api(`/api/admin/audit?${params}`).catch(() => null);
    if (mine !== seq) return;
    if (res?.ok) result = (await res.json()) as AuditPage;
    else error = 'Could not load the audit log.';
  }

  // Back to the first page whenever the filter changes.
  $effect(() => {
    void filter;
    untrack(() => {
      page = 1;
      void load();
    });
  });

  // Something changed elsewhere: re-read where we are. Skips the first run,
  // which the effect above already covers.
  let seen = untrack(() => version);
  $effect(() => {
    if (version === seen) return;
    seen = version;
    untrack(load);
  });

  function go(next: number) {
    page = next;
    void load();
  }

  const REGISTRATION: Record<string, string> = { true: 'open', false: 'closed', null: 'server default' };

  /** What happened, in words. `target` is rendered separately, as a filter link. */
  function describe(e: AuditEntry): { before: string; after: string } {
    const d = e.detail;
    switch (e.action) {
      case 'user.disable':
        return { before: 'Disabled', after: d.reason ? `: ${d.reason}` : '' };
      case 'user.enable':
        return { before: 'Enabled', after: '' };
      // The command line's role changes.
      case 'user.grant_admin':
        return { before: 'Made', after: ' an admin' };
      case 'user.revoke_admin':
        return { before: 'Removed admin from', after: '' };
      case 'user.set_role':
        return d.to === 'admin'
          ? { before: 'Made', after: ' an admin' }
          : { before: 'Removed admin from', after: '' };
      case 'user.revoke_sessions':
        return { before: 'Signed out', after: ` everywhere (${d.count} session${d.count === 1 ? '' : 's'})` };
      case 'user.revoke_token':
        return { before: `Revoked token “${d.token_name}” of`, after: '' };
      case 'user.delete': {
        const n = Array.isArray(d.released_carriers) ? d.released_carriers.length : 0;
        return { before: 'Deleted', after: n ? ` (released ${(d.released_carriers as string[]).join(', ')})` : '' };
      }
      case 'carrier.hide':
        return { before: 'Hid carrier', after: d.reason ? `: ${d.reason}` : '' };
      case 'carrier.unhide':
        return { before: 'Unhid carrier', after: '' };
      case 'carrier.reassign':
        return { before: 'Gave carrier', after: ` from ${d.owner ?? 'nobody'} to ${d.to_owner}` };
      case 'carrier.release':
        return { before: 'Released carrier', after: d.owner ? ` from ${d.owner}` : '' };
      case 'carrier.trim':
        return { before: 'Trimmed', after: `'s history before ${d.before} (${d.events} event${d.events === 1 ? '' : 's'})` };
      case 'carrier.delete':
        return { before: 'Deleted carrier', after: ` (${d.events} event${d.events === 1 ? '' : 's'})` };
      case 'settings.registration':
        return {
          before: `Set registration to ${REGISTRATION[String(d.to)]}`,
          after: d.to === null ? ` (${d.env ? 'open' : 'closed'})` : ''
        };
      default:
        return { before: e.action, after: '' };
    }
  }

  function actor(e: AuditEntry): string {
    if (e.admin) return e.admin;
    return e.detail.via === 'cli' ? 'command line' : 'deleted account';
  }
</script>

<section class="section">
  <div class="history__head">
    <h2>Audit log</h2>
    {#if result}<span class="status status--online">{result.total} entr{result.total === 1 ? 'y' : 'ies'}</span>{/if}
  </div>

  {#if filter}
    <p class="option__hint">
      Showing actions on <strong>{filter.name}</strong> only{filter.kind === 'user' ? ', and on carriers it owned' : ''}.
      <button onclick={() => onfilter(null)}>Show all</button>
    </p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {:else if !result}
    <p class="muted">Loading…</p>
  {:else if result.items.length === 0}
    <p class="muted">Nothing logged yet.</p>
  {:else}
    <div class="table-wrap">
      <table class="admin-audit">
        <thead>
          <tr><th>When</th><th>By</th><th>Action</th></tr>
        </thead>
        <tbody>
          {#each result.items as e (e.id)}
            {@const text = describe(e)}
            <tr>
              <td>{formatTimestamp(e.created_at)}</td>
              <td>
                {#if e.admin_id !== null}
                  <button class="link" onclick={() => onopenuser(e.admin_id!)}>{actor(e)}</button>
                {:else}
                  <span class="muted">{actor(e)}</span>
                {/if}
              </td>
              <!-- One line: whitespace between the parts would show in the text. -->
              <td>{text.before}{#if e.target_id !== null}{' '}<button class="link" title="Only this account's entries" onclick={() => onfilter({ kind: e.target_kind!, id: e.target_id!, name: e.target ?? `#${e.target_id}` })}>{e.target ?? `#${e.target_id}`}</button>{/if}{text.after}</td>
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
