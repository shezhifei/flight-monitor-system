// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';

type Recorded = { method: string; url: string; body?: unknown };
const recorded: Recorded[] = [];

function makeResponse(_ok: boolean, status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

async function dispatch(method: string, url: string, body?: unknown) {
  const rec: Recorded = { method, url, body };
  recorded.push(rec);
  if (method === 'GET' && url.includes('/metadata-catalogs/icao_size')) {
    return {
      ok: true,
      status: 200,
      data: {
        data: {
          code: 'icao_size',
          name: 'ICAO 机位等级',
          is_open: false,
          is_ordered: true,
          system_owned: true,
          is_active: true,
          entries: [{ catalog_code: 'icao_size', code: 'C', name: 'C', rank: 3, is_active: true, source: 'manual' }],
        },
      },
      response: makeResponse(true, 200, {}),
    };
  }
  if (method === 'GET' && url.includes('/metadata-catalogs')) {
    return {
      ok: true,
      status: 200,
      data: {
        data: [
          {
            code: 'icao_size',
            name: 'ICAO 机位等级',
            is_open: false,
            is_ordered: true,
            system_owned: true,
            is_active: true,
          },
        ],
      },
      response: makeResponse(true, 200, {}),
    };
  }
  if (method === 'POST' && url.endsWith('/entries')) {
    return { ok: true, status: 201, data: { data: { code: 'G' } }, response: makeResponse(true, 201, {}) };
  }
  return { ok: true, status: 200, data: { data: {} }, response: makeResponse(true, 200, {}) };
}

vi.mock('@/composables/useApi', () => ({
  useApi: () => ({
    get: (url: string) => dispatch('GET', url),
    post: (url: string, body?: unknown) => dispatch('POST', url, body),
    patch: (url: string, body?: unknown) => dispatch('PATCH', url, body),
  }),
}));

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    show: vi.fn(),
    showToast: vi.fn(),
    toasts: { value: [] },
  }),
}));

import { useMetadataCatalog } from '@/composables/useMetadataCatalog';

describe('useMetadataCatalog', () => {
  beforeEach(() => {
    recorded.length = 0;
  });

  it('loads catalogs then entries for the first row', async () => {
    const mc = useMetadataCatalog();
    await mc.loadCatalogs();
    expect(recorded.some((r) => r.method === 'GET' && r.url.includes('/metadata-catalogs?include_inactive=true'))).toBe(
      true,
    );
    expect(mc.catalogs.value[0]?.code).toBe('icao_size');
    expect(mc.detail.value?.entries[0]?.code).toBe('C');
  });

  it('posts a new entry to the selected catalog', async () => {
    const mc = useMetadataCatalog();
    await mc.loadCatalogs();
    mc.openEntryModal();
    mc.entryForm.value = { code: 'G', name: 'G', rank: '7' };
    await mc.saveCurrentModal();
    const post = recorded.find((r) => r.method === 'POST' && String(r.url).endsWith('/entries'));
    expect(post?.body).toEqual({ code: 'G', name: 'G', rank: 7 });
  });
});
