<script lang="ts">
	import { resolveDownloadFilename } from '$lib/content-disposition';

	// backend の POST /api/v1/saddle-stitch (中綴じ製本用の PDF 変換API) を叩く画面。
	// バイナリレスポンスのヘッダーをそのまま扱いたいので、型付きクライアント (openapi-fetch) では
	// なく生の fetch を使う。
	let files = $state<FileList | undefined>(undefined);
	let direction = $state<'left' | 'right'>('left');
	let submitting = $state(false);
	let errorMessage = $state('');

	const selectedFile = $derived(files?.[0]);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		if (!selectedFile || submitting) {
			return;
		}

		submitting = true;
		errorMessage = '';

		try {
			const formData = new FormData();
			formData.append('file', selectedFile);
			formData.append('direction', direction);

			const response = await fetch('/api/v1/saddle-stitch', {
				method: 'POST',
				body: formData
			});

			if (!response.ok) {
				errorMessage = await resolveErrorMessage(response);
				return;
			}

			const blob = await response.blob();
			const filename = resolveDownloadFilename(response.headers.get('content-disposition'));
			downloadBlob(blob, filename);
		} catch {
			errorMessage = '通信エラーが発生しました。ネットワーク接続を確認してください。';
		} finally {
			submitting = false;
		}
	}

	async function resolveErrorMessage(response: Response): Promise<string> {
		try {
			const body = await response.json();
			if (body?.error?.message) {
				return body.error.message as string;
			}
		} catch {
			// JSON でないレスポンスは無視してデフォルトメッセージを使う
		}
		return `変換に失敗しました (status: ${response.status})`;
	}

	function downloadBlob(blob: Blob, filename: string) {
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = filename;
		document.body.appendChild(anchor);
		anchor.click();
		anchor.remove();
		// クリック直後に revoke するとブラウザによってはダウンロードが失敗することがあるため、
		// 次のマクロタスクまで遅らせる。
		setTimeout(() => URL.revokeObjectURL(url), 0);
	}
</script>

<h1>中綴じ製本 PDF 変換</h1>

<form onsubmit={handleSubmit}>
	<div class="field">
		<label for="file-input">PDFファイル</label>
		<input id="file-input" type="file" accept="application/pdf" bind:files />
	</div>

	<fieldset class="field">
		<legend>開き方向</legend>
		<label>
			<input type="radio" name="direction" bind:group={direction} value="left" />
			左開き
		</label>
		<label>
			<input type="radio" name="direction" bind:group={direction} value="right" />
			右開き
		</label>
	</fieldset>

	<button type="submit" disabled={!selectedFile || submitting}>
		{submitting ? '変換中...' : '変換する'}
	</button>

	{#if errorMessage}
		<p class="error" role="alert">{errorMessage}</p>
	{/if}
</form>

<style>
	form {
		display: flex;
		max-width: 28rem;
		flex-direction: column;
		gap: 1rem;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0;
		padding: 0;
		border: none;
	}

	fieldset.field {
		padding: 0.5rem 1rem 1rem;
		border: 1px solid #ccc;
		border-radius: 4px;
	}

	fieldset.field label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: normal;
	}

	button {
		align-self: flex-start;
		padding: 0.5rem 1.5rem;
	}

	.error {
		color: #b00020;
	}
</style>
