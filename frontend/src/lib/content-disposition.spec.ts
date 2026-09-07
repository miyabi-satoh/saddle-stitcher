import { describe, it, expect } from 'vitest';
import { resolveDownloadFilename } from './content-disposition';

describe('resolveDownloadFilename', () => {
	it("decodes a filename from filename*=UTF-8''...", () => {
		const header = 'attachment; filename="output.pdf"; filename*=UTF-8\'\'%E5%87%BA%E5%8A%9B.pdf';
		expect(resolveDownloadFilename(header)).toBe('出力.pdf');
	});

	it('falls back to the default filename when the header is missing', () => {
		expect(resolveDownloadFilename(null)).toBe('output.pdf');
		expect(resolveDownloadFilename(undefined)).toBe('output.pdf');
		expect(resolveDownloadFilename('')).toBe('output.pdf');
	});

	it('falls back to the default filename when the value is malformed', () => {
		const header = "attachment; filename*=UTF-8''%E5%87";
		expect(resolveDownloadFilename(header)).toBe('output.pdf');
	});
});
