<script lang="ts">
	let { children } = $props();
	import '$lib/styles/global.css';
</script>

<svelte:head>
</svelte:head>

{@render children()}
