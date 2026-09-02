// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';

// Mock useApi BEFORE importing the composable (Vitest hoists vi.mock).
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

import { splitConflictMessage, useTerminalDirectory } from '@/composables/useTerminalDirectory';

const T1 = { terminal_id: 'term_t1', code: 'T1', name: '一号航站楼', is_active: true };

const T1_CONTEXT = {
  terminal: T1,
  stands: [{ id: 'stand_a12', code: 'A12', name: null, is_active: true }],
  gates: [{ gate_id: 'gate_ga01', code: 'G-A01', name: 'A01口', is_active: true }],
  carousels: [{ carousel_id: 'car_b1', code: 'B1', name: null, is_active: true }],
};

function setupContextResponder() {
  responders.push(rec => {
    if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/terminals?')) {
      return { ok: true, status: 200, data: [T1], response: makeResponse(true, 200, []) };
    }
    if (rec.method === 'GET' && rec.url === '/api/v2/dispatch/terminals/term_t1/context') {
      return { ok: true, status: 200, data: { success: true, data: T1_CONTEXT }, response: makeResponse(true, 200, {}) };
    }
    return null;
  });
}

beforeEach(() => {
  recorded.length = 0;
  responders.length = 0;
  toastCalls.length = 0;
});

describe('splitConflictMessage', () => {
  it('splits base message and parses JSON detail array', () => {
    const { base, details } = splitConflictMessage(
      '停用楼失败：存在未结束占用/分配; 明细: [{"stand_code":"A12","flight":"CA1234"}]',
    );
    expect(base).toBe('停用楼失败：存在未结束占用/分配');
    expect(details).toEqual([{ stand_code: 'A12', flight: 'CA1234' }]);
  });

  it('keeps original message when no marker present', () => {
    const { base, details } = splitConflictMessage('普通错误');
    expect(base).toBe('普通错误');
    expect(details).toEqual([]);
  });

  it('falls back to original message when details are not valid JSON', () => {
    const { base, details } = splitConflictMessage('停用失败; 明细: not-json');
    expect(base).toBe('停用失败; 明细: not-json');
    expect(details).toEqual([]);
  });
});

describe('useTerminalDirectory', () => {
  it('fetches terminals with include_inactive and loads context on select', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();
    await td.fetchTerminals();
    expect(recorded.some(r => r.method === 'GET' && r.url === '/api/v2/dispatch/terminals?include_inactive=true')).toBe(true);
    expect(td.terminals.value.map(t => t.code)).toEqual(['T1']);

    await td.selectTerminal('term_t1');
    expect(td.directory.value?.terminal.code).toBe('T1');
    expect(td.directory.value?.stands.map(s => s.code)).toEqual(['A12']);
    expect(td.directory.value?.gates.map(g => g.code)).toEqual(['G-A01']);
    expect(td.directory.value?.carousels.map(c => c.code)).toEqual(['B1']);
  });

  it('createTerminal POSTs code+name and refreshes list', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();
    await td.createTerminal({ code: 'T2', name: '二号航站楼', attributes: {} });
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/terminals');
    expect(post?.body).toEqual({ code: 'T2', name: '二号航站楼', attributes: {} });
    expect(recorded.some(r => r.method === 'GET' && r.url.startsWith('/api/v2/dispatch/terminals?'))).toBe(true);
  });

  it('createStand requires a selected terminal and POSTs terminal_id', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();
    recorded.length = 0;
    const fail = await td.createStand({
      code: 'C03',
      name: '',
      area: '',
      stand_type: '',
      size_category: '',
      attributes: {},
    });
    expect(fail).toBe(false);
    expect(recorded.some(r => r.method === 'POST' && r.url === '/api/v2/dispatch/stands')).toBe(false);

    await td.selectTerminal('term_t1');
    recorded.length = 0;
    const ok = await td.createStand({
      code: 'C03',
      name: '远机位',
      area: '远',
      stand_type: '',
      size_category: 'C',
      attributes: {},
    });
    expect(ok).toBe(true);
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/stands');
    // size_category 旧列只读：新建不再提交；attributes（含 overlay 值）整体提交。
    expect(post?.body).toEqual({
      terminal_id: 'term_t1',
      code: 'C03',
      name: '远机位',
      area: '远',
      stand_type: null,
      attributes: {},
    });
  });

  it('createGate requires a selected terminal and POSTs terminal_id', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();
    recorded.length = 0;
    const fail = await td.createGate({ code: 'G-B01', name: '', attributes: {} });
    expect(fail).toBe(false);
    expect(recorded.some(r => r.method === 'POST' && r.url === '/api/v2/dispatch/gates')).toBe(false);

    await td.selectTerminal('term_t1');
    recorded.length = 0;
    const ok = await td.createGate({ code: 'G-B01', name: 'B01口', attributes: {} });
    expect(ok).toBe(true);
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/gates');
    expect(post?.body).toEqual({ terminal_id: 'term_t1', code: 'G-B01', name: 'B01口', attributes: {} });
  });

  it('attach/detach stand hit the member routes', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();
    await td.selectTerminal('term_t1');
    recorded.length = 0;

    td.attachStandId.value = 'stand_c03';
    await td.attachStand();
    expect(
      recorded.some(r => r.method === 'POST' && r.url === '/api/v2/dispatch/terminals/term_t1/stands/stand_c03'),
    ).toBe(true);

    await td.detachStand('stand_a12');
    expect(
      recorded.some(r => r.method === 'DELETE' && r.url === '/api/v2/dispatch/terminals/stands/stand_a12'),
    ).toBe(true);
  });

  it('deactivateTerminal on 409 opens conflict modal with parsed occupancy details', async () => {
    setupContextResponder();
    responders.push(rec => {
      if (rec.method === 'POST' && rec.url === '/api/v2/dispatch/terminals/term_t1/deactivate') {
        const body = {
          success: false,
          error: {
            message: '停用楼失败：存在未结束占用/分配; 明细: [{"stand_code":"A12","flight_no":"CA1234"}]',
          },
        };
        return { ok: false, status: 409, data: body, response: makeResponse(false, 409, body) };
      }
      return null;
    });
    const td = useTerminalDirectory();
    const ok = await td.deactivateTerminal('term_t1');
    expect(ok).toBe(false);
    expect(td.modal.value.kind).toBe('conflict');
    if (td.modal.value.kind === 'conflict') {
      expect(td.modal.value.message).toBe('停用楼失败：存在未结束占用/分配');
      expect(td.modal.value.details).toEqual([{ stand_code: 'A12', flight_no: 'CA1234' }]);
    }
    expect(toastCalls.some(t => t.type === 'error')).toBe(false);
  });

  it('detachStand on 409 opens conflict modal instead of toast', async () => {
    setupContextResponder();
    responders.push(rec => {
      if (rec.method === 'DELETE' && rec.url === '/api/v2/dispatch/terminals/stands/stand_a12') {
        const body = {
          success: false,
          error: { message: '移出机位失败：存在未结束占用; 明细: [{"stand_code":"A12"}]' },
        };
        return { ok: false, status: 409, data: body, response: makeResponse(false, 409, body) };
      }
      return null;
    });
    const td = useTerminalDirectory();
    await td.selectTerminal('term_t1');
    const ok = await td.detachStand('stand_a12');
    expect(ok).toBe(false);
    expect(td.modal.value.kind).toBe('conflict');
  });

  it('attachableStands excludes stands already in the directory', async () => {
    setupContextResponder();
    responders.push(rec => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/resources/stands')) {
        return {
          ok: true,
          status: 200,
          data: [
            { id: 'stand_a12', code: 'A12' },
            { id: 'stand_c03', code: 'C03' },
          ],
          response: makeResponse(true, 200, []),
        };
      }
      return null;
    });
    const td = useTerminalDirectory();
    await td.selectTerminal('term_t1');
    await td.fetchAllStands();
    expect(td.attachableStands.value.map(s => s.id)).toEqual(['stand_c03']);
  });

  it('openStandModal seeds combined_stand=false and loads stored attributes', async () => {
    setupContextResponder();
    const td = useTerminalDirectory();

    // 新建：未触碰时 combined_stand 显式为 false，composed_of 不会先显示。
    td.openStandModal();
    expect(td.standForm.value.attributes).toEqual({ combined_stand: false });

    // 编辑：已存值优先于种子。
    td.openStandModal({
      id: 'stand_316',
      code: '316',
      attributes: { combined_stand: true, stand_use: 'remote', composed_of: ['316L', '316R'] },
    });
    expect(td.standForm.value.attributes).toEqual({
      combined_stand: true,
      stand_use: 'remote',
      composed_of: ['316L', '316R'],
    });

    // 已存 combined_stand=false 时保留 false（不是缺省）。
    td.openStandModal({ id: 'stand_c03', code: 'C03', attributes: { stand_use: 'remote' } });
    expect(td.standForm.value.attributes).toEqual({ combined_stand: false, stand_use: 'remote' });
  });
});
