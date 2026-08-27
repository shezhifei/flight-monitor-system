// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';

type Recorded = { method: string; url: string; body?: unknown };
const recorded: Recorded[] = [];
const responders: Array<
  (r: Recorded) =>
    | Promise<{ ok: boolean; status: number; data: unknown; response: Response }>
    | { ok: boolean; status: number; data: unknown; response: Response }
    | null
> = [];

function makeResponse(_ok: boolean, status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function defaultResponder(rec: Recorded) {
  if (rec.method === 'GET') {
    return { ok: true, status: 200, data: [], response: makeResponse(true, 200, []) };
  }
  return { ok: true, status: 200, data: null, response: makeResponse(true, 200, null) };
}

async function dispatch(method: string, url: string, body?: unknown) {
  const rec: Recorded = { method, url, body };
  recorded.push(rec);
  for (let i = responders.length - 1; i >= 0; i--) {
    const out = await responders[i](rec);
    if (out) return out;
  }
  return defaultResponder(rec);
}

vi.mock('@/composables/useApi', () => ({
  useApi: () => ({
    raw: vi.fn(),
    request: vi.fn(),
    get: (url: string) => dispatch('GET', url),
    post: (url: string, body?: unknown) => dispatch('POST', url, body),
    put: (url: string, body?: unknown) => dispatch('PUT', url, body),
    patch: (url: string, body?: unknown) => dispatch('PATCH', url, body),
    delete: (url: string) => dispatch('DELETE', url),
  }),
}));

const toastCalls: Array<{ type: string; message: string }> = [];
vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    showToast: (type: string, message: unknown) => {
      toastCalls.push({ type, message: String(message) });
    },
    show: (type: string, message: unknown) => {
      toastCalls.push({ type, message: String(message) });
    },
    toasts: { value: [] },
  }),
}));

import { useQualificationCatalog } from '@/composables/useQualificationCatalog';

const CATALOG = {
  id: 'q1',
  department_id: 'dept_1',
  qualification_code: 'TOWING',
  qualification_name: '牵引',
  is_active: true,
};

function setupResponder() {
  responders.push(rec => {
    if (rec.method === 'GET' && rec.url.includes('/qualifications')) {
      return { ok: true, status: 200, data: [CATALOG], response: makeResponse(true, 200, []) };
    }
    if (rec.method === 'GET' && rec.url.includes('/qualification-levels')) {
      return {
        ok: true,
        status: 200,
        data: [
          {
            id: 'l1',
            department_id: 'dept_1',
            qualification_code: 'TOWING',
            level_code: 'senior',
            level_name: '高级',
            level_rank: 2,
            is_active: true,
          },
        ],
        response: makeResponse(true, 200, []),
      };
    }
    return null;
  });
}

describe('useQualificationCatalog', () => {
  beforeEach(() => {
    recorded.length = 0;
    responders.length = 0;
    toastCalls.length = 0;
  });

  it('selectDepartment loads catalogs and levels', async () => {
    setupResponder();
    const qc = useQualificationCatalog();
    await qc.selectDepartment('dept_1');
    expect(qc.catalogs.value.map(c => c.qualification_code)).toEqual(['TOWING']);
    expect(qc.levelsFor('TOWING').map(l => l.level_code)).toEqual(['senior']);
  });

  it('create qualification POSTs to department rules', async () => {
    setupResponder();
    const qc = useQualificationCatalog();
    await qc.selectDepartment('dept_1');
    recorded.length = 0;
    qc.openQualificationModal();
    qc.form.value = { qualification_code: 'WATER', qualification_name: '加水', description: '' };
    await qc.saveCurrentModal();
    const post = recorded.find(r => r.method === 'POST' && r.url.endsWith('/qualifications'));
    expect(post?.body).toEqual({
      qualification_code: 'WATER',
      qualification_name: '加水',
      description: null,
      is_active: true,
    });
  });
});
