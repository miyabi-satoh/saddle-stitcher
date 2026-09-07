import createClient from 'openapi-fetch';
import type { paths } from '$lib/api/schema';

// openapi.json のパスは `/api/v1/...` を含む (backend 側で nest した prefix がそのまま
// ドキュメントに載る) ため、baseUrl は空で良い。
export const client = createClient<paths>({ baseUrl: '' });
