<script lang="ts">
	import { onMount } from 'svelte';
	import { client } from '$lib/api/client';

	// backend の /api/v1/health を叩けることを確認するための最小限のデモ。
	// SvelteKit(SPA) → axum(rust-embed で埋め込み) の配線が動いているかの動作確認用。
	let status = $state<'loading' | 'ok' | 'error'>('loading');
	let version = $state('');

	onMount(async () => {
		try {
			const { data, response } = await client.GET('/api/v1/health');
			if (response.ok && data) {
				status = 'ok';
				version = data.version;
			} else {
				status = 'error';
			}
		} catch {
			status = 'error';
		}
	});
</script>

<h1>Welcome to SvelteKit</h1>
<p>Visit <a href="https://svelte.dev/docs/kit">svelte.dev/docs/kit</a> to read the documentation</p>

<p>
	backend:
	{#if status === 'loading'}
		確認中...
	{:else if status === 'ok'}
		起動しています (v{version})
	{:else}
		応答がありません (`make dev-backend` は起動していますか?)
	{/if}
</p>
