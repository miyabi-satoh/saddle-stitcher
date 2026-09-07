import { expect, test } from '@playwright/test';

test('selects a PDF, converts it, and downloads the result', async ({ page }) => {
	await page.route('**/api/v1/saddle-stitch', async (route) => {
		await route.fulfill({
			status: 200,
			headers: {
				'content-type': 'application/pdf',
				'content-disposition':
					'attachment; filename="output.pdf"; filename*=UTF-8\'\'%E5%87%BA%E5%8A%9B.pdf'
			},
			body: Buffer.from('%PDF-1.4 fake output pdf')
		});
	});

	await page.goto('/');

	await page.setInputFiles('input[type="file"]', {
		name: 'input.pdf',
		mimeType: 'application/pdf',
		buffer: Buffer.from('%PDF-1.4 fake input pdf')
	});

	const [download] = await Promise.all([
		page.waitForEvent('download'),
		page.getByRole('button', { name: '変換する' }).click()
	]);

	expect(download.suggestedFilename()).toBe('出力.pdf');
});
