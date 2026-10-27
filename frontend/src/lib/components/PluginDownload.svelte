<script lang="ts">
  import { onMount } from 'svelte';
  import { formatNumber } from '$lib/format';

  /** The EDMC plugin as served by this backend, and how to set it up. */
  interface PluginInfo {
    name: string;
    version: string;
    file: string;
    size: number;
  }

  let info = $state<PluginInfo | null>(null);
  let failed = $state(false);
  // The plugin posts to the same origin as this page: behind the proxy (and in
  // dev, through vite) `/api` is served here.
  const url = typeof location === 'undefined' ? '' : location.origin;

  onMount(async () => {
    const res = await fetch('/api/plugin').catch(() => null);
    if (res?.ok) info = (await res.json()) as PluginInfo;
    else failed = true;
  });
</script>

<section class="section">
  <div class="history__head">
    <h2>EDMC plugin</h2>
    {#if info}<span class="status status--online">v{info.version}</span>{/if}
  </div>

  {#if failed}
    <p class="error">The plugin download is unavailable right now.</p>
  {:else if info}
    <p class="muted">
      Sends your carrier's journal events and market to this site through
      <a href="https://github.com/EDCD/EDMarketConnector" target="_blank" rel="noreferrer">EDMarketConnector</a>.
    </p>
    <div class="form__actions">
      <a class="button" href="/api/plugin/download" download={info.file}>
        Download {info.file} ({formatNumber(Math.ceil(info.size / 1024))} KB)
      </a>
    </div>
    <ol class="steps">
      <li>
        In EDMC open <em>File → Settings → Plugins</em> and click <em>Open</em>. Unzip the download
        there, so the <code>{info.name}</code> folder sits in the plugins folder.
      </li>
      <li>Restart EDMC.</li>
      <li>
        In <em>File → Settings → {info.name}</em>, set the URL to
        <input class="steps__copy" readonly value={url} onfocus={(e) => e.currentTarget.select()} />
        and paste a token from below.
      </li>
      <li>Open Carrier Management in game: the plugin's first CarrierStats claims your carrier.</li>
    </ol>
    <p class="muted">Updating: replace the folder with a newer download; your URL and token are kept.</p>
  {:else}
    <p class="muted">Loading…</p>
  {/if}
</section>
