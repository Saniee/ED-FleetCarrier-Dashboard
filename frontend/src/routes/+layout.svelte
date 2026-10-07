<script lang="ts">
	let { children } = $props();
	import '$lib/styles/global.scss';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/auth.svelte';
</script>

<svelte:head>
</svelte:head>

<div class="carrier-page">
	<div class="carrier_header">
		<div>
			<button onclick={() => goto('/')}>Carriers</button>
		</div>
		<div>
			{#if auth.user}
				<button onclick={() => goto('/account')}>{auth.user.username}</button>
				<button onclick={() => auth.logout()}>Logout</button>
			{:else}
				<button onclick={() => goto('/login')}>Login</button>
			{/if}
		</div>
	</div>
</div>

{@render children()}
