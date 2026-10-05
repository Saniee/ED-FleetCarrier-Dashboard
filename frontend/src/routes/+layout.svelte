<script lang="ts">
	let { children } = $props();
	import '$lib/styles/global.scss';
</script>

<svelte:head>
</svelte:head>

{@render children()}
