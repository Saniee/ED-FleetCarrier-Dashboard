<script lang="ts">
	let { children } = $props();
	import '$lib/styles/global.scss';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/auth.svelte';
	import { theme } from '$lib/theme.svelte';
</script>

<svelte:head>
</svelte:head>

<nav class="carrier-page site-nav">
	<div class="carrier_header">
		<div>
			<button onclick={() => goto('/')}>Carriers</button>
		</div>
		<div>
			<button onclick={() => theme.toggle()}>Change Theme</button>
			{#if auth.user}
				<button onclick={() => goto('/account')}>{auth.user.username}</button>
				<button onclick={() => auth.logout()}>Logout</button>
			{:else}
				<button onclick={() => goto('/login')}>Login</button>
			{/if}
		</div>
	</div>
</nav>

{@render children()}
