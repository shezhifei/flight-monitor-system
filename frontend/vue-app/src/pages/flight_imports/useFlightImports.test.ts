// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, nextTick } from 'vue';

type Recorded = { method: string; url: string; body?: unknown };
const recorded: Recorded[] = [];
const responders: Array<
  (r: Recorded) =>
    | Promise<{ ok: boolean; status: number; data: unknown; response: Response } | null>
    | { ok: boolean; status: number; data: unknown; response: Response }
    | null
> = [];

function makeResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function result(ok: boolean, status: number, data: unknown) {
  return { ok, status, data, response: makeResponse(status, data) };
}

async function dispatch(method: string, url: string, body?: unknown) {
  const rec: Recorded = { method, url, body };
  recorded.push(rec);
  for (let i = responders.length - 1; i >= 0; i--) {
    const out = await responders[i](rec);
    if (out) return out;
  }
  return result(true, 200, null);
}

vi.mock('@/composables/useApi', () => ({
  useApi: () => ({
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

import {
  flightImportSnapshotFromApi,
  useFlightImports,
} from '@/composables/useFlightImports';

const samplePreview = {
  preview_id: 'prev-001',
  airport_context: { icao: 'ZGSZ' },
  source_file: { filename: 'PAYLOAD.txt', size: 128, checksum_sha256: 'abc' },
  summary: {
    total_rows: 2,
    valid_rows: 1,
    invalid_rows: 1,
    create_count: 1,
    update_count: 0,
    skip_count: 0,
    failed_count: 0,
    warning_count: 1,
    error_count: 1,
  },
  rows: [
    {
      source_row_key: 'row-1',
      match_strategy: 'none',
      matched_flight_id: null,
      action: 'create',
      normalized_flight: { flight_number: 'CA100' },
      timeline_events: [],
      warnings: ['stand ambiguous'],
      errors: [],
    },
    {
      source_row_key: 'row-2',
      match_strategy: 'none',
      action: 'skip',
      normalized_flight: {},
      warnings: [],
      errors: ['missing schedule'],
    },
  ],
  errors: [],
  mapping_version: 'v1',
  status: 'previewed',
  field_mapping: { FLIGHT_NO: 'flight_number' },
  created_at: '2026-07-14T00:00:00Z',
  expires_at: '2026-07-14T01:00:00Z',
  source_system: 'payload',
};

async function mountComposable() {
  let api!: ReturnType<typeof useFlightImports>;
  const Host = defineComponent({
    setup() {
      api = useFlightImports();
      return () => null;
    },
  });
  const { createApp } = await import('vue');
  const app = createApp(Host);
  app.mount(document.createElement('div'));
  await nextTick();
  return { api, unmount: () => app.unmount() };
}

beforeEach(() => {
  recorded.length = 0;
  responders.length = 0;
  toastCalls.length = 0;
  window.history.replaceState({}, '', '/frontend/flight_imports.html');
});

describe('flightImportSnapshotFromApi', () => {
  it('maps backend preview schema fields', () => {
    const snap = flightImportSnapshotFromApi(samplePreview);
    expect(snap?.preview_id).toBe('prev-001');
    expect(snap?.summary.invalid_rows).toBe(1);
    expect(snap?.field_mapping).toEqual({ FLIGHT_NO: 'flight_number' });
    expect(snap?.airport_context).toEqual({ icao: 'ZGSZ' });
    expect(snap?.source_file?.filename).toBe('PAYLOAD.txt');
    expect(snap?.rows[1]?.errors).toEqual(['missing schedule']);
  });
});

describe('useFlightImports', () => {
  it('issues exactly one preview request per explicit preview() call', async () => {
    responders.push(rec => {
      if (rec.method === 'POST' && rec.url === '/api/v2/system/flight-imports/preview') {
        return result(true, 200, { ...samplePreview, summary: { ...samplePreview.summary, invalid_rows: 0 }, rows: [samplePreview.rows[0]] });
      }
      return null;
    });
    const { api, unmount } = await mountComposable();
    recorded.length = 0;
    const file = new File(['ok'], 'PAYLOAD.txt', { type: 'text/plain' });
    await api.preview(file);
    await api.preview(file);
    expect(recorded.filter(r => r.url === '/api/v2/system/flight-imports/preview')).toHaveLength(2);
    expect(api.previewId.value).toBe('prev-001');
    expect(api.status.value).toBe('previewed');
    expect(api.fieldMapping.value).toEqual({ FLIGHT_NO: 'flight_number' });
    unmount();
  });

  it('disables commit when invalid_rows > 0', async () => {
    responders.push(rec => {
      if (rec.url === '/api/v2/system/flight-imports/preview') {
        return result(true, 200, samplePreview);
      }
      return null;
    });
    const { api, unmount } = await mountComposable();
    await api.preview(new File(['x'], 'PAYLOAD.txt'));
    expect(api.summary.value.invalid_rows).toBe(1);
    expect(api.canCommit.value).toBe(false);
    recorded.length = 0;
    await api.commitImport();
    expect(recorded.some(r => r.url.includes('/commit'))).toBe(false);
    unmount();
  });

  it('allows commit when status is previewed and invalid_rows is 0', async () => {
    const clean = {
      ...samplePreview,
      summary: { ...samplePreview.summary, invalid_rows: 0, error_count: 0 },
      rows: [{ ...samplePreview.rows[0], errors: [] }],
      errors: [],
    };
    responders.push(rec => {
      if (rec.url === '/api/v2/system/flight-imports/preview') {
        return result(true, 200, clean);
      }
      if (rec.url === '/api/v2/system/flight-imports/prev-001/commit') {
        return result(true, 200, {
          ...clean,
          status: 'committed',
          committed_at: '2026-07-14T00:10:00Z',
          flight_ids: ['f1'],
          summary: { ...clean.summary, create_count: 1 },
        });
      }
      if (rec.url === '/api/v2/system/flight-imports/prev-001/result') {
        return result(true, 200, {
          ...clean,
          status: 'committed',
          committed_at: '2026-07-14T00:10:00Z',
          flight_ids: ['f1'],
        });
      }
      return null;
    });
    const { api, unmount } = await mountComposable();
    await api.preview(new File(['x'], 'PAYLOAD.txt'));
    expect(api.canCommit.value).toBe(true);
    await api.commitImport();
    expect(api.status.value).toBe('committed');
    // committed result stays visible
    expect(api.snapshot.value).toBeTruthy();
    expect(api.previewData.value.length).toBeGreaterThan(0);
    expect(api.fileSelected.value).toBe(true);
    unmount();
  });

  it('loads preview_id from URL and fetches result for terminal status', async () => {
    window.history.replaceState({}, '', '/frontend/flight_imports.html?preview_id=prev-url');
    responders.push(rec => {
      if (rec.url === '/api/v2/system/flight-imports/prev-url') {
        return result(true, 200, { ...samplePreview, preview_id: 'prev-url', status: 'committed' });
      }
      if (rec.url === '/api/v2/system/flight-imports/prev-url/result') {
        return result(true, 200, {
          ...samplePreview,
          preview_id: 'prev-url',
          status: 'committed',
          flight_ids: ['f9'],
        });
      }
      return null;
    });
    const { api, unmount } = await mountComposable();
    // onMounted load may still be in flight
    await nextTick();
    await nextTick();
    await Promise.resolve();
    await Promise.resolve();
    expect(recorded.some(r => r.url === '/api/v2/system/flight-imports/prev-url')).toBe(true);
    expect(recorded.some(r => r.url === '/api/v2/system/flight-imports/prev-url/result')).toBe(true);
    expect(api.previewId.value).toBe('prev-url');
    expect(api.status.value).toBe('committed');
    unmount();
  });

  it('keeps prior preview when commit fails', async () => {
    const clean = {
      ...samplePreview,
      summary: { ...samplePreview.summary, invalid_rows: 0, error_count: 0 },
      rows: [{ ...samplePreview.rows[0], errors: [] }],
      errors: [],
    };
    responders.push(rec => {
      if (rec.url === '/api/v2/system/flight-imports/preview') return result(true, 200, clean);
      if (rec.url.includes('/commit')) return result(false, 500, { message: '导入事务失败' });
      return null;
    });
    const { api, unmount } = await mountComposable();
    await api.preview(new File(['x'], 'PAYLOAD.txt'));
    await api.commitImport();
    expect(api.importProgress.value).toBe(0);
    expect(api.fileSelected.value).toBe(true);
    expect(api.previewData.value.length).toBe(1);
    expect(toastCalls).toContainEqual({ type: 'error', message: '导入事务失败' });
    unmount();
  });

  it('reset clears snapshot, URL preview_id, and optional file input', async () => {
    const { api, unmount } = await mountComposable();
    api.previewId.value = 'prev-x';
    api.fileSelected.value = true;
    api.previewData.value = [{ source_row_key: 'r', action: 'create' }];
    window.history.replaceState({}, '', '/frontend/flight_imports.html?preview_id=prev-x');
    const input = document.createElement('input');
    input.type = 'file';
    // jsdom cannot set FileList easily; value clear is enough for our reset contract
    api.reset({ clearFileInput: input });
    expect(api.previewId.value).toBe('');
    expect(api.fileSelected.value).toBe(false);
    expect(api.previewData.value).toEqual([]);
    expect(window.location.search).not.toContain('preview_id');
    unmount();
  });
});
