const DEFAULT_FILENAME = 'output.pdf';

/**
 * `Content-Disposition` レスポンスヘッダーからダウンロードファイル名を解決する。
 *
 * バックエンドは `attachment; filename="output.pdf"; filename*=UTF-8''<percent-encoded>` の
 * 形式でヘッダーを付与する。日本語ファイル名は `filename="..."` 側では文字化けし得るため、
 * 常に `filename*=UTF-8''...` (RFC 5987) 側からデコードする。ヘッダーが無い場合や
 * デコードに失敗した場合はフォールバック名を返す。
 */
export function resolveDownloadFilename(
	contentDisposition: string | null | undefined,
	fallback = DEFAULT_FILENAME
): string {
	if (!contentDisposition) {
		return fallback;
	}

	const match = contentDisposition.match(/filename\*\s*=\s*UTF-8''([^;]+)/i);
	if (!match) {
		return fallback;
	}

	try {
		const decoded = decodeURIComponent(match[1].trim());
		return decoded || fallback;
	} catch {
		return fallback;
	}
}
