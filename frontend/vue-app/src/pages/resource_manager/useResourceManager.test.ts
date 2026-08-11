// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { defineComponent, nextTick } from 'vue';

// Mock useApi BEFORE importing the composable (Vitest hoists vi.mock).
type Recorded = { method: string; url: string; body?: unknown };
const recorded: Recorded[] = [];
const responders: Array<(r: Recorded) => Promise<{ ok: boolean; status: number; data: unknown; response: Response }> | { ok: boolean; status: number; data: unknown; response: Response } | null> = [];

function makeResponse(_ok: boolean, status: number, body: unknown): Response {
  return new Response(JSON.stringify(body ?? null), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function defaultResponder(rec: Recorded) {
  // GET /teams etc. return empty arrays so onMounted does not blow up
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

vi.mock('@/composables/useAuth', () => ({
  useAuth: () => ({
    fetch: vi.fn(),
    getUser: () => null,
    isAdmin: () => false,
    logout: vi.fn(),
    initialize: vi.fn(),
  }),
}));

// Capture toast calls
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
  equipmentFromApi,
  equipmentToCreateApi,
  teamFromApi,
  teamToCreateApi,
  teamTypeToApi,
  useResourceManager,
} from '@/composables/useResourceManager';

function mountComposable() {
  let api!: ReturnType<typeof useResourceManager>;
  const Host = defineComponent({
    setup() {
      api = useResourceManager();
      return () => null;
    },
  });
  // Mount via createApp to fire onMounted
  return import('vue').then(({ createApp }) => {
    const app = createApp(Host);
    const root = document.createElement('div');
    app.mount(root);
    return { api, unmount: () => app.unmount() };
  });
}

beforeEach(() => {
  recorded.length = 0;
  responders.length = 0;
  toastCalls.length = 0;
});

describe('fromApi/toApi mappers', () => {
  it('maps team fixture fields with backend names', () => {
    const team = teamFromApi({
      id: 'team-ground-01',
      name: '地服一组',
      team_type_id: 'team-type-driver',
      code: 'GROUND-01',
      leader_id: '00000000-0000-4000-8000-000000000002',
      terminal: 'T1',
      current_status: 'available',
      current_stand_id: 'stand-a12',
      member_count: 3,
      is_active: true,
    });
    expect(team).toMatchObject({
      id: 'team-ground-01',
      team_type_id: 'team-type-driver',
      current_status: 'available',
      current_stand_id: 'stand-a12',
      leader_id: '00000000-0000-4000-8000-000000000002',
    });
  });

  it('maps equipment fixture fields including required code', () => {
    const eq = equipmentFromApi({
      id: 'equipment-tug-01',
      code: 'TUG-01',
      equipment_type_id: 'equipment-type-tug',
      name: '一号牵引车',
      license_plate: '粤A·FMS01',
      terminal: 'T1',
      status: 'available',
      current_stand_id: 'stand-a12',
      next_maintenance_date: '2026-08-01',
      equipment_type_name: '牵引车',
    });
    expect(eq).toMatchObject({
      code: 'TUG-01',
      equipment_type_id: 'equipment-type-tug',
      equipment_type_name: '牵引车',
      license_plate: '粤A·FMS01',
      next_maintenance_date: '2026-08-01',
    });
  });

  it('equipment create payload requires code and backend field names', () => {
    expect(
      equipmentToCreateApi({
        code: 'TUG-02',
        name: '二号牵引车',
        equipment_type_id: 'equipment-type-tug',
        license_plate: '粤A·FMS02',
        terminal: 'T2',
        status: 'available',
        next_maintenance_date: '2026-09-01',
      }),
    ).toEqual({
      code: 'TUG-02',
      name: '二号牵引车',
      equipment_type_id: 'equipment-type-tug',
      license_plate: '粤A·FMS02',
      terminal: 'T2',
      next_maintenance_date: '2026-09-01',
    });
  });

  it('team create payload uses team_type_id / leader_id / terminal', () => {
    expect(
      teamToCreateApi({
        name: '地服二组',
        code: 'GROUND-02',
        team_type_id: 'team-type-driver',
        terminal: 'T1',
        leader_id: 'u1',
        current_status: 'available',
      }),
    ).toEqual({
      name: '地服二组',
      code: 'GROUND-02',
      team_type_id: 'team-type-driver',
      terminal: 'T1',
      leader_id: 'u1',
    });
  });
});

describe('useResourceManager', () => {
  it('bootstraps with teams/team-types/equipment-types/users/me endpoints and page params', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    await nextTick();
    const urls = recorded.map(r => r.url);
    expect(urls.some(u => u.startsWith('/api/v2/dispatch/teams'))).toBe(true);
    expect(urls.some(u => u.startsWith('/api/v2/dispatch/team-types'))).toBe(true);
    expect(urls.some(u => u.startsWith('/api/v2/dispatch/equipment-types'))).toBe(true);
    expect(urls.some(u => u.startsWith('/api/v2/auth/users'))).toBe(true);
    expect(urls).toContain('/api/v2/auth/me');
    const teamsUrl = urls.find(u => u.startsWith('/api/v2/dispatch/teams'));
    expect(teamsUrl).toContain('include_inactive=false');
    expect(teamsUrl).toContain('page_size=');
    expect(api.modal.value.kind).toBe('none');
    unmount();
  });

  it('maps list responses with backend field names and server totals', async () => {
    responders.push(rec => {
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/teams')) {
        return {
          ok: true,
          status: 200,
          data: {
            success: true,
            data: [
              {
                id: 'team-ground-01',
                name: '地服一组',
                team_type_id: 'team-type-driver',
                code: 'GROUND-01',
                leader_id: 'u2',
                terminal: 'T1',
                current_status: 'on_duty',
                current_stand_id: 'stand-a12',
                member_count: 3,
                is_active: true,
              },
            ],
          },
          response: makeResponse(true, 200, {}),
        };
      }
      if (rec.method === 'GET' && rec.url.startsWith('/api/v2/dispatch/equipment') && !rec.url.includes('equipment-types')) {
        return {
          ok: true,
          status: 200,
          data: {
            success: true,
            data: [
              {
                id: 'equipment-tug-01',
                code: 'TUG-01',
                equipment_type_id: 'equipment-type-tug',
                name: '一号牵引车',
                license_plate: '粤A·FMS01',
                status: 'available',
                current_stand_id: 'stand-a12',
                next_maintenance_date: '2026-08-01',
                equipment_type_name: '牵引车',
              },
            ],
          },
          response: makeResponse(true, 200, {}),
        };
      }
      return null;
    });
    const { api, unmount } = await mountComposable();
    await nextTick();
    await nextTick();
    expect(api.rawTeams.value[0]?.team_type_id).toBe('team-type-driver');
    expect(api.rawTeams.value[0]?.current_status).toBe('on_duty');
    expect(api.teamsTotal.value).toBe(1);
    await api.fetchEquipment();
    expect(api.rawEquipment.value[0]?.license_plate).toBe('粤A·FMS01');
    expect(api.rawEquipment.value[0]?.equipment_type_name).toBe('牵引车');
    expect(api.equipmentTotal.value).toBe(1);
    unmount();
  });

  it('uses discriminated modal kinds (not plate_number heuristics)', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    api.openTeamTypeModal({ id: 'tt1', name: '机务班组', code: 'MX' });
    expect(api.modal.value.kind).toBe('team-type');
    api.openEquipmentTypeModal({ id: 'et1', name: '牵引车', code: 'TUG' });
    expect(api.modal.value.kind).toBe('equipment-type');
    api.openEquipmentStatusModal({ id: 'eq1', code: 'EQ-1', name: '车辆01', license_plate: 'ABC' });
    expect(api.modal.value.kind).toBe('equipment-status');
    api.openTeamModal({ id: 't1', name: 'A班' });
    expect(api.modal.value.kind).toBe('team');
    api.openEquipmentModal({ id: 'eq2', code: 'EQ-2', name: '设备2', license_plate: 'XYZ' });
    expect(api.modal.value.kind).toBe('equipment');
    api.closeModal();
    expect(api.modal.value.kind).toBe('none');
    unmount();
  });

  it('createTeamType posts task_types parsed from comma-separated input', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    api.teamTypeForm.value = {
      name: '机务班组',
      code: 'MX',
      department_id: '',
      color: '#007AFF',
      is_driver_type: false,
      task_types: 'boarding, cleaning',
      description: '描述',
    };
    responders.push(rec => {
      if (rec.method === 'POST' && rec.url === '/api/v2/dispatch/team-types') {
        return { ok: true, status: 201, data: { id: 'tt_1' }, response: makeResponse(true, 201, {}) };
      }
      return null;
    });
    const ok = await api.createTeamType(api.teamTypeForm.value);
    expect(ok).toBe(true);
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/team-types');
    expect(post).toBeTruthy();
    expect(post?.body).toEqual(teamTypeToApi(api.teamTypeForm.value));
    expect(post?.body).toEqual({
      name: '机务班组',
      code: 'MX',
      department_id: null,
      description: '描述',
      color: '#007AFF',
      is_driver_type: false,
      task_types: ['boarding', 'cleaning'],
    });
    unmount();
  });

  it('updateTeamType PUTs to /team-types/{id}', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    await api.updateTeamType('tt_1', {
      name: '新名',
      code: '',
      department_id: '',
      color: '',
      is_driver_type: false,
      task_types: '',
      description: '',
    });
    const put = recorded.find(r => r.method === 'PUT' && r.url === '/api/v2/dispatch/team-types/tt_1');
    expect(put).toBeTruthy();
    expect((put?.body as { name?: string })?.name).toBe('新名');
    unmount();
  });

  it('deleteTeamType DELETEs and refreshes', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    await api.deleteTeamType('tt_1');
    expect(recorded.some(r => r.method === 'DELETE' && r.url === '/api/v2/dispatch/team-types/tt_1')).toBe(true);
    expect(recorded.some(r => r.method === 'GET' && r.url.startsWith('/api/v2/dispatch/team-types'))).toBe(true);
    unmount();
  });

  it('createEquipmentType includes driver_team_type_id and icon', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    await api.createEquipmentType({
      name: '牵引车',
      code: 'TUG',
      category: 'vehicle',
      requires_driver: true,
      driver_team_type_id: 'team-type-driver',
      icon: 'tractor',
      description: '',
    });
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/equipment-types');
    expect(post?.body).toEqual({
      name: '牵引车',
      code: 'TUG',
      category: 'vehicle',
      requires_driver: true,
      driver_team_type_id: 'team-type-driver',
      icon: 'tractor',
      description: null,
    });
    unmount();
  });

  it('createEquipment includes required code field', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    const ok = await api.createEquipment({
      code: 'TUG-01',
      name: '一号牵引车',
      equipment_type_id: 'equipment-type-tug',
      license_plate: '粤A·FMS01',
      terminal: 'T1',
      status: 'available',
      next_maintenance_date: '2026-08-01',
    });
    expect(ok).toBe(true);
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/equipment');
    expect(post?.body).toEqual({
      code: 'TUG-01',
      name: '一号牵引车',
      equipment_type_id: 'equipment-type-tug',
      license_plate: '粤A·FMS01',
      terminal: 'T1',
      next_maintenance_date: '2026-08-01',
    });
    unmount();
  });

  it('rejects equipment create without code', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    toastCalls.length = 0;
    const ok = await api.createEquipment({
      code: '',
      name: '无代码',
      equipment_type_id: '',
      license_plate: '',
      terminal: '',
      status: 'available',
      next_maintenance_date: '',
    });
    expect(ok).toBe(false);
    expect(recorded.some(r => r.method === 'POST' && r.url === '/api/v2/dispatch/equipment')).toBe(false);
    expect(toastCalls.some(t => t.type === 'warning')).toBe(true);
    unmount();
  });

  it('updateEquipmentStatus uses legacy PUT /equipment/{id}/status?status=...', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    await api.updateEquipmentStatus('eq_1', {
      status: 'maintenance',
      terminal: '机库A',
      next_maintenance_date: '2026-07-01',
    });
    const statusCall = recorded.find(r => r.method === 'PUT' && r.url.startsWith('/api/v2/dispatch/equipment/eq_1/status'));
    expect(statusCall).toBeTruthy();
    expect(statusCall?.url).toContain('status=maintenance');
    const metaCall = recorded.find(r => r.method === 'PUT' && r.url === '/api/v2/dispatch/equipment/eq_1');
    expect(metaCall).toBeTruthy();
    expect((metaCall?.body as { terminal?: string }).terminal).toBe('机库A');
    expect((metaCall?.body as { next_maintenance_date?: string }).next_maintenance_date).toBe('2026-07-01');
    unmount();
  });

  it('loads team members and adds via POST /teams/{id}/members', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    responders.push(rec => {
      if (rec.method === 'GET' && rec.url === '/api/v2/dispatch/teams/team_1/members') {
        return {
          ok: true,
          status: 200,
          data: [{ user_id: 'u1', username: 'alice', user_display_name: 'Alice', role: 'member' }],
          response: makeResponse(true, 200, []),
        };
      }
      return null as ReturnType<(typeof responders)[number]>;
    });
    await api.openTeamMembersDrawer({ id: 'team_1', name: 'A班' });
    expect(api.teamMembers.value).toHaveLength(1);
    expect(api.teamMembers.value[0]?.user_display_name).toBe('Alice');
    expect(api.modal.value.kind).toBe('team-members');

    recorded.length = 0;
    api.teamMemberAdd.value = { user_id: 'u2', role: 'leader', can_drive: true };
    await api.addTeamMember('team_1');
    const post = recorded.find(r => r.method === 'POST' && r.url === '/api/v2/dispatch/teams/team_1/members');
    expect(post).toBeTruthy();
    expect(post?.body).toEqual({ user_id: 'u2', role: 'leader', can_drive: true });
    expect(recorded.some(r => r.method === 'GET' && r.url.startsWith('/api/v2/dispatch/teams'))).toBe(true);
    unmount();
  });

  it('removeTeamMember calls DELETE /teams/{tid}/members/{uid}', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    await api.removeTeamMember('team_1', 'u9');
    expect(recorded.some(r => r.method === 'DELETE' && r.url === '/api/v2/dispatch/teams/team_1/members/u9')).toBe(true);
    unmount();
  });

  it('shows backend error verbatim when delete is blocked', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    toastCalls.length = 0;
    responders.push(rec => {
      if (rec.method === 'DELETE' && rec.url.startsWith('/api/v2/dispatch/team-types/')) {
        return {
          ok: false,
          status: 409,
          data: { message: '存在引用，禁止删除' },
          response: makeResponse(false, 409, { message: '存在引用，禁止删除' }),
        };
      }
      return null as ReturnType<(typeof responders)[number]>;
    });
    const ok = await api.deleteTeamType('tt_blocked');
    expect(ok).toBe(false);
    const errToast = toastCalls.find(t => t.type === 'error');
    expect(errToast?.message).toBe('存在引用，禁止删除');
    unmount();
  });

  it('local filters respect independent per-section state', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    api.rawTeams.value = [
      { id: 't1', name: 'Alpha', team_type_id: 'tt1', current_status: 'on_duty' },
      { id: 't2', name: 'Bravo', team_type_id: 'tt2', current_status: 'off_duty' },
    ];
    api.rawEquipment.value = [
      { id: 'e1', code: 'E1', name: 'Tug', equipment_type_id: 'et1', license_plate: 'ABC', status: 'available' },
      { id: 'e2', code: 'E2', name: 'Loader', equipment_type_id: 'et2', status: 'maintenance' },
    ];
    api.rawTeamTypes.value = [{ id: 'tt1', name: '机务' }, { id: 'tt2', name: '保洁' }];
    api.rawEquipmentTypes.value = [
      { id: 'et1', name: '牵引车', code: 'TUG' },
      { id: 'et2', name: '装载机', code: 'LDR' },
    ];

    api.teamSearch.value = 'alp';
    expect(api.teams.value.map(t => t.id)).toEqual(['t1']);
    api.teamSearch.value = '';
    api.teamTypeFilter.value = 'tt2';
    expect(api.teams.value.map(t => t.id)).toEqual(['t2']);
    api.teamTypeFilter.value = '';

    api.equipmentSearch.value = 'abc';
    expect(api.equipment.value.map(e => e.id)).toEqual(['e1']);
    api.equipmentSearch.value = '';
    api.equipmentStatusFilter.value = 'maintenance';
    expect(api.equipment.value.map(e => e.id)).toEqual(['e2']);

    api.teamTypeSearch.value = '机';
    expect(api.teamTypes.value.map(t => t.id)).toEqual(['tt1']);
    api.equipmentTypeSearch.value = 'ldr';
    expect(api.equipmentTypes.value.map(t => t.id)).toEqual(['et2']);
    unmount();
  });

  it('saveCurrentModal dispatches by kind, never by plate_number', async () => {
    const { api, unmount } = await mountComposable();
    await nextTick();
    recorded.length = 0;
    api.openTeamTypeModal({ id: 'tt_99', name: '保洁班', code: 'CLN' });
    api.teamTypeForm.value.name = '保洁班';
    await api.saveCurrentModal();
    expect(recorded.some(r => r.method === 'PUT' && r.url === '/api/v2/dispatch/team-types/tt_99')).toBe(true);
    expect(recorded.some(r => r.url === '/api/v2/dispatch/teams/tt_99')).toBe(false);
    expect(recorded.some(r => r.url === '/api/v2/dispatch/equipment/tt_99')).toBe(false);
    unmount();
  });
});
