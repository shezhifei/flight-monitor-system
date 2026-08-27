import { computed, onMounted, ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

// ----------------------------------------------------------------------------
// Typed models — backend field names (dispatch_schemas.rs)
// ----------------------------------------------------------------------------

export type ResourceSection =
  | 'teams'
  | 'team-types'
  | 'equipment'
  | 'equipment-types'
  | 'departments'
  | 'qualifications'
  | 'terminals';

export interface Team {
  id: string;
  name: string;
  /** 只读历史值：班组类型已降为只读目录（PR2） */
  team_type_id?: string | null;
  /** PR2 起班组直接挂科室 */
  department_id?: string | null;
  code?: string | null;
  leader_id?: string | null;
  leader_name?: string | null;
  current_status?: string | null;
  current_stand_id?: string | null;
  member_count?: number;
  is_active?: boolean;
  /** Resolved client-side from team types */
  team_type_name?: string | null;
  team_type_color?: string | null;
  /** Resolved client-side from departments */
  department_name?: string | null;
}

export interface Department {
  id: string;
  name: string;
  code?: string | null;
  description?: string | null;
  manager_id?: string | null;
  is_active?: boolean;
  /** Resolved client-side from assignable users */
  manager_name?: string | null;
}

export interface TeamMember {
  id?: string;
  team_id?: string;
  user_id: string;
  username?: string | null;
  user_display_name?: string | null;
  role?: string;
  can_drive?: boolean;
  joined_at?: string | null;
  is_active?: boolean;
}

export interface TeamType {
  id: string;
  name: string;
  code?: string | null;
  department_id?: string | null;
  description?: string | null;
  color?: string | null;
  is_driver_type?: boolean;
  task_types?: string[];
  is_active?: boolean;
  team_count?: number;
}

export interface Equipment {
  id: string;
  code: string;
  name?: string | null;
  equipment_type_id?: string | null;
  equipment_type_name?: string | null;
  /** PR2 起设备直接挂科室（无常驻楼字段） */
  department_id?: string | null;
  license_plate?: string | null;
  status?: string | null;
  current_stand_id?: string | null;
  next_maintenance_date?: string | null;
  is_active?: boolean;
  /** Resolved client-side from departments */
  department_name?: string | null;
}

export interface EquipmentType {
  id: string;
  name: string;
  code?: string | null;
  category?: string | null;
  requires_driver?: boolean;
  icon?: string | null;
  description?: string | null;
  is_active?: boolean;
  equipment_count?: number;
}

export interface AssignableUser {
  id: string;
  username?: string;
  display_name?: string;
  email?: string;
  is_active?: boolean;
}

export interface SidebarUser {
  username: string;
  initial: string;
  role: string;
  is_admin: boolean;
}

// ----------------------------------------------------------------------------
// Discriminated modal state
// ----------------------------------------------------------------------------

export type ResourceModal =
  | { kind: 'none' }
  | { kind: 'team'; item?: Team }
  | { kind: 'equipment'; item?: Equipment }
  | { kind: 'team-type'; item?: TeamType }
  | { kind: 'equipment-type'; item?: EquipmentType }
  | { kind: 'equipment-status'; item: Equipment }
  | { kind: 'department'; item?: Department }
  | { kind: 'team-members'; team: Team };

export interface TeamFormData {
  name: string;
  code: string;
  department_id: string;
  leader_id: string;
  current_status: string;
}

export interface EquipmentFormData {
  code: string;
  name: string;
  equipment_type_id: string;
  department_id: string;
  license_plate: string;
  status: string;
  next_maintenance_date: string;
}

export interface DepartmentFormData {
  name: string;
  code: string;
  description: string;
  manager_id: string;
}

export interface TeamTypeFormData {
  name: string;
  code: string;
  department_id: string;
  color: string;
  is_driver_type: boolean;
  task_types: string;
  description: string;
}

export interface EquipmentTypeFormData {
  name: string;
  code: string;
  category: string;
  requires_driver: boolean;
  icon: string;
  description: string;
}

export interface EquipmentStatusFormData {
  status: string;
  next_maintenance_date: string;
}

const DEFAULT_PAGE_SIZE = 200;

// ----------------------------------------------------------------------------
// fromApi / toApi mappers
// ----------------------------------------------------------------------------

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : null;
}

function asString(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return null;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim()) {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return undefined;
}

function asBool(value: unknown): boolean {
  return Boolean(value);
}

function asStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.map(v => asString(v)).filter((v): v is string => Boolean(v));
}

export function teamFromApi(raw: unknown): Team | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const name = asString(r.name);
  if (!id || !name) return null;
  return {
    id,
    name,
    team_type_id: asString(r.team_type_id),
    department_id: asString(r.department_id),
    code: asString(r.code),
    leader_id: asString(r.leader_id),
    leader_name: asString(r.leader_name),
    current_status: asString(r.current_status) ?? 'available',
    current_stand_id: asString(r.current_stand_id),
    member_count: asNumber(r.member_count) ?? 0,
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function teamToCreateApi(form: TeamFormData) {
  return {
    name: form.name.trim(),
    code: form.code.trim() || null,
    department_id: form.department_id.trim(),
    leader_id: form.leader_id.trim() || null,
  };
}

export function teamToUpdateApi(form: TeamFormData) {
  return {
    name: form.name.trim(),
    code: form.code.trim() || null,
    department_id: form.department_id.trim() || null,
    leader_id: form.leader_id.trim() || null,
    current_status: form.current_status.trim() || null,
  };
}

export function teamMemberFromApi(raw: unknown): TeamMember | null {
  const r = asRecord(raw);
  if (!r) return null;
  const user_id = asString(r.user_id);
  if (!user_id) return null;
  return {
    id: asString(r.id) ?? undefined,
    team_id: asString(r.team_id) ?? undefined,
    user_id,
    username: asString(r.username),
    user_display_name: asString(r.user_display_name) ?? asString(r.display_name),
    role: asString(r.role) ?? 'member',
    can_drive: asBool(r.can_drive),
    joined_at: asString(r.joined_at),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function teamTypeFromApi(raw: unknown): TeamType | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const name = asString(r.name);
  if (!id || !name) return null;
  return {
    id,
    name,
    code: asString(r.code),
    department_id: asString(r.department_id),
    description: asString(r.description),
    color: asString(r.color),
    is_driver_type: asBool(r.is_driver_type),
    task_types: asStringArray(r.task_types),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
    team_count: asNumber(r.team_count),
  };
}

export function teamTypeToApi(form: TeamTypeFormData) {
  const task_types = form.task_types
    .split(/[,，\s]+/)
    .map(s => s.trim())
    .filter(Boolean);
  return {
    name: form.name.trim(),
    code: form.code.trim() || null,
    department_id: form.department_id.trim() || null,
    description: form.description.trim() || null,
    color: form.color.trim() || null,
    is_driver_type: form.is_driver_type,
    task_types,
  };
}

export function equipmentFromApi(raw: unknown): Equipment | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const code = asString(r.code);
  if (!id || !code) return null;
  return {
    id,
    code,
    name: asString(r.name),
    equipment_type_id: asString(r.equipment_type_id),
    equipment_type_name: asString(r.equipment_type_name),
    department_id: asString(r.department_id),
    license_plate: asString(r.license_plate),
    status: asString(r.status) ?? 'available',
    current_stand_id: asString(r.current_stand_id),
    next_maintenance_date: asString(r.next_maintenance_date),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function equipmentToCreateApi(form: EquipmentFormData) {
  return {
    code: form.code.trim(),
    name: form.name.trim() || null,
    equipment_type_id: form.equipment_type_id.trim() || null,
    department_id: form.department_id.trim(),
    license_plate: form.license_plate.trim() || null,
    next_maintenance_date: form.next_maintenance_date.trim() || null,
  };
}

export function equipmentToUpdateApi(form: EquipmentFormData) {
  return {
    code: form.code.trim() || null,
    name: form.name.trim() || null,
    equipment_type_id: form.equipment_type_id.trim() || null,
    department_id: form.department_id.trim() || null,
    license_plate: form.license_plate.trim() || null,
    status: form.status.trim() || null,
    next_maintenance_date: form.next_maintenance_date.trim() || null,
  };
}

export function equipmentTypeFromApi(raw: unknown): EquipmentType | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const name = asString(r.name);
  if (!id || !name) return null;
  return {
    id,
    name,
    code: asString(r.code),
    category: asString(r.category),
    requires_driver: asBool(r.requires_driver),
    icon: asString(r.icon),
    description: asString(r.description),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
    equipment_count: asNumber(r.equipment_count),
  };
}

export function equipmentTypeToApi(form: EquipmentTypeFormData) {
  return {
    name: form.name.trim(),
    code: form.code.trim() || null,
    category: form.category.trim() || null,
    requires_driver: form.requires_driver,
    icon: form.icon.trim() || null,
    description: form.description.trim() || null,
  };
}

export function departmentFromApi(raw: unknown): Department | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const name = asString(r.name);
  if (!id || !name) return null;
  return {
    id,
    name,
    code: asString(r.code),
    description: asString(r.description),
    manager_id: asString(r.manager_id),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function departmentToApi(form: DepartmentFormData) {
  return {
    name: form.name.trim(),
    code: form.code.trim() || null,
    description: form.description.trim() || null,
    manager_id: form.manager_id.trim() || null,
  };
}

export function assignableUserFromApi(raw: unknown): AssignableUser | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  if (!id) return null;
  if (r.is_active === false) return null;
  return {
    id,
    username: asString(r.username) ?? undefined,
    display_name: asString(r.display_name) ?? asString(r.effective_operator_name) ?? undefined,
    email: asString(r.email) ?? undefined,
    is_active: true,
  };
}

// ----------------------------------------------------------------------------
// Response unwrapping helpers.
// API responses may be either bare arrays/objects or wrapped { success, data }.
// ----------------------------------------------------------------------------

function unwrap<T>(payload: unknown): T | null {
  if (payload && typeof payload === 'object' && 'data' in (payload as Record<string, unknown>)) {
    return ((payload as Record<string, unknown>).data ?? null) as T | null;
  }
  return (payload ?? null) as T | null;
}

function unwrapListRaw(payload: unknown): unknown[] {
  const data = unwrap<unknown[] | { items?: unknown[]; total?: number }>(payload);
  if (Array.isArray(data)) return data;
  if (data && typeof data === 'object' && Array.isArray((data as { items?: unknown[] }).items)) {
    return (data as { items: unknown[] }).items;
  }
  return [];
}

function unwrapTotal(payload: unknown, fallback: number): number {
  if (payload && typeof payload === 'object') {
    const root = payload as Record<string, unknown>;
    const nested = root.data && typeof root.data === 'object' ? (root.data as Record<string, unknown>) : root;
    for (const key of ['total', 'total_count', 'count']) {
      const n = asNumber(nested[key]);
      if (n !== undefined) return n;
    }
  }
  return fallback;
}

async function extractErrorMessage(response: Response, fallback: string): Promise<string> {
  try {
    const ct = String(response.headers.get('content-type') || '').toLowerCase();
    if (ct.includes('application/json')) {
      const body = (await response.clone().json()) as {
        message?: string;
        detail?: string;
        error?: string | { message?: string };
      };
      // 真实错误体为 { success:false, error:{ message, ... } }；兼容旧的扁平 message/detail。
      if (typeof body.error === 'object' && body.error?.message) return body.error.message;
      if (typeof body.error === 'string' && body.error) return body.error;
      return body.message || body.detail || fallback;
    }
    const text = await response.clone().text();
    return text.trim() || fallback;
  } catch {
    return fallback;
  }
}

function pageQuery(extra: Record<string, string | boolean | number | undefined> = {}): string {
  const params = new URLSearchParams();
  params.set('page', '1');
  params.set('page_size', String(DEFAULT_PAGE_SIZE));
  for (const [k, v] of Object.entries(extra)) {
    if (v === undefined || v === null || v === '') continue;
    params.set(k, String(v));
  }
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

// ----------------------------------------------------------------------------
// Composable
// ----------------------------------------------------------------------------

export interface ResourceManagerOptions {
  loadAssignableUsers?: boolean;
}

export function useResourceManager(options: ResourceManagerOptions = {}) {
  const api = useApi();
  const toast = useToast();

  const activeSection = ref<ResourceSection>('teams');
  const loading = ref(false);
  const saving = ref(false);

  const teams = ref<Team[]>([]);
  const equipment = ref<Equipment[]>([]);
  const teamTypes = ref<TeamType[]>([]);
  const equipmentTypes = ref<EquipmentType[]>([]);
  const departments = ref<Department[]>([]);
  const assignableUsers = ref<AssignableUser[]>([]);

  // server-reported / page totals
  const teamsTotal = ref(0);
  const equipmentTotal = ref(0);
  const teamTypesTotal = ref(0);
  const equipmentTypesTotal = ref(0);
  const departmentsTotal = ref(0);

  // independent filter state per section
  const teamSearch = ref('');
  const teamTypeFilter = ref('');
  const teamStatusFilter = ref('');

  const equipmentSearch = ref('');
  const equipmentTypeFilter = ref('');
  const equipmentStatusFilter = ref('');

  const teamTypeSearch = ref('');
  const equipmentTypeSearch = ref('');
  const departmentSearch = ref('');

  // discriminated modal state
  const modal = ref<ResourceModal>({ kind: 'none' });

  // form state (one per kind)
  const teamForm = ref<TeamFormData>({
    name: '',
    code: '',
    department_id: '',
    leader_id: '',
    current_status: 'available',
  });
  const equipmentForm = ref<EquipmentFormData>({
    code: '',
    name: '',
    equipment_type_id: '',
    department_id: '',
    license_plate: '',
    status: 'available',
    next_maintenance_date: '',
  });
  const teamTypeForm = ref<TeamTypeFormData>({
    name: '',
    code: '',
    department_id: '',
    color: '',
    is_driver_type: false,
    task_types: '',
    description: '',
  });
  const equipmentTypeForm = ref<EquipmentTypeFormData>({
    name: '',
    code: '',
    category: '',
    requires_driver: false,
    icon: '',
    description: '',
  });
  const equipmentStatusForm = ref<EquipmentStatusFormData>({
    status: 'available',
    next_maintenance_date: '',
  });
  const departmentForm = ref<DepartmentFormData>({
    name: '',
    code: '',
    description: '',
    manager_id: '',
  });

  // team member drawer state
  const teamMembers = ref<TeamMember[]>([]);
  const teamMembersLoading = ref(false);
  const teamMemberAdd = ref<{ user_id: string; role: string; can_drive: boolean }>({
    user_id: '',
    role: 'member',
    can_drive: false,
  });
  const teamMemberAddBusy = ref(false);
  const memberSearch = ref('');

  // sidebar user
  const sidebarUser = ref<SidebarUser>({ username: '加载中...', initial: 'A', role: '管理员', is_admin: false });

  function enrichTeam(team: Team): Team {
    const type = teamTypes.value.find(t => t.id === team.team_type_id);
    const leader = assignableUsers.value.find(u => u.id === team.leader_id);
    const dept = departments.value.find(d => d.id === team.department_id);
    return {
      ...team,
      team_type_name: type?.name ?? team.team_type_name ?? null,
      team_type_color: type?.color ?? team.team_type_color ?? null,
      leader_name: team.leader_name || leader?.display_name || leader?.username || null,
      department_name: dept?.name ?? team.department_name ?? null,
    };
  }

  function enrichEquipment(eq: Equipment): Equipment {
    const type = equipmentTypes.value.find(t => t.id === eq.equipment_type_id);
    const dept = departments.value.find(d => d.id === eq.department_id);
    return {
      ...eq,
      equipment_type_name: eq.equipment_type_name ?? type?.name ?? null,
      department_name: dept?.name ?? eq.department_name ?? null,
    };
  }

  function enrichDepartment(dept: Department): Department {
    const manager = assignableUsers.value.find(u => u.id === dept.manager_id);
    return {
      ...dept,
      manager_name: dept.manager_name || manager?.display_name || manager?.username || null,
    };
  }

  // ------------- filters (local computed) -------------------

  const filteredTeams = computed(() => {
    const q = teamSearch.value.trim().toLowerCase();
    const tf = teamTypeFilter.value;
    const sf = teamStatusFilter.value;
    return teams.value
      .map(enrichTeam)
      .filter(t => {
        if (tf && (t.team_type_id ?? '') !== tf) return false;
        if (sf && (t.current_status ?? '') !== sf) return false;
        if (!q) return true;
        const hay = [t.name, t.code, t.leader_name].filter(Boolean).join(' ').toLowerCase();
        return hay.includes(q);
      });
  });

  const filteredEquipment = computed(() => {
    const q = equipmentSearch.value.trim().toLowerCase();
    const tf = equipmentTypeFilter.value;
    const sf = equipmentStatusFilter.value;
    return equipment.value
      .map(enrichEquipment)
      .filter(e => {
        if (tf && (e.equipment_type_id ?? '') !== tf) return false;
        if (sf && (e.status ?? '') !== sf) return false;
        if (!q) return true;
        const hay = [e.name, e.code, e.license_plate].filter(Boolean).join(' ').toLowerCase();
        return hay.includes(q);
      });
  });

  const filteredTeamTypes = computed(() => {
    const q = teamTypeSearch.value.trim().toLowerCase();
    if (!q) return teamTypes.value;
    return teamTypes.value.filter(t => {
      const hay = [t.name, t.code, ...(t.task_types ?? [])].filter(Boolean).join(' ').toLowerCase();
      return hay.includes(q);
    });
  });

  const filteredEquipmentTypes = computed(() => {
    const q = equipmentTypeSearch.value.trim().toLowerCase();
    if (!q) return equipmentTypes.value;
    return equipmentTypes.value.filter(t => {
      const hay = [t.name, t.code, t.category, t.icon].filter(Boolean).join(' ').toLowerCase();
      return hay.includes(q);
    });
  });

  const filteredDepartments = computed(() => {
    const q = departmentSearch.value.trim().toLowerCase();
    return departments.value.map(enrichDepartment).filter(d => {
      if (!q) return true;
      const hay = [d.name, d.code, d.description, d.manager_name].filter(Boolean).join(' ').toLowerCase();
      return hay.includes(q);
    });
  });

  const filteredAssignableUsers = computed(() => {
    const taken = new Set(teamMembers.value.map(m => m.user_id));
    const q = memberSearch.value.trim().toLowerCase();
    return assignableUsers.value.filter(u => {
      if (taken.has(u.id)) return false;
      if (!q) return true;
      const hay = [u.username, u.display_name, u.email].filter(Boolean).join(' ').toLowerCase();
      return hay.includes(q);
    });
  });

  // ------------- fetchers ----------------------------------

  async function fetchTeams() {
    loading.value = true;
    try {
      const query: Record<string, string | boolean | number | undefined> = {
        include_inactive: false,
      };
      if (teamTypeFilter.value) query.team_type_id = teamTypeFilter.value;
      const res = await api.get<unknown>(`/api/v2/dispatch/teams${pageQuery(query)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '加载班组失败'));
        return;
      }
      const list = unwrapListRaw(res.data)
        .map(teamFromApi)
        .filter((t): t is Team => Boolean(t));
      teams.value = list;
      teamsTotal.value = unwrapTotal(res.data, list.length);
    } finally {
      loading.value = false;
    }
  }

  async function fetchEquipment() {
    loading.value = true;
    try {
      const query: Record<string, string | boolean | number | undefined> = {
        include_inactive: false,
      };
      if (equipmentTypeFilter.value) query.equipment_type_id = equipmentTypeFilter.value;
      if (equipmentStatusFilter.value) query.status = equipmentStatusFilter.value;
      const res = await api.get<unknown>(`/api/v2/dispatch/equipment${pageQuery(query)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '加载设备失败'));
        return;
      }
      const list = unwrapListRaw(res.data)
        .map(equipmentFromApi)
        .filter((e): e is Equipment => Boolean(e));
      equipment.value = list;
      equipmentTotal.value = unwrapTotal(res.data, list.length);
    } finally {
      loading.value = false;
    }
  }

  async function fetchTeamTypes() {
    const res = await api.get<unknown>(`/api/v2/dispatch/team-types${pageQuery({ include_inactive: false })}`);
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '加载班组类型失败'));
      return;
    }
    const list = unwrapListRaw(res.data)
      .map(teamTypeFromApi)
      .filter((t): t is TeamType => Boolean(t));
    teamTypes.value = list;
    teamTypesTotal.value = unwrapTotal(res.data, list.length);
  }

  async function fetchEquipmentTypes() {
    const res = await api.get<unknown>(`/api/v2/dispatch/equipment-types${pageQuery({ include_inactive: false })}`);
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '加载设备类型失败'));
      return;
    }
    const list = unwrapListRaw(res.data)
      .map(equipmentTypeFromApi)
      .filter((t): t is EquipmentType => Boolean(t));
    equipmentTypes.value = list;
    equipmentTypesTotal.value = unwrapTotal(res.data, list.length);
  }

  async function fetchDepartments(includeInactive = false) {
    const res = await api.get<unknown>(
      `/api/v2/dispatch/resources/departments${pageQuery({ include_inactive: includeInactive })}`,
    );
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '加载科室失败'));
      return;
    }
    const list = unwrapListRaw(res.data)
      .map(departmentFromApi)
      .filter((d): d is Department => Boolean(d));
    departments.value = list;
    departmentsTotal.value = unwrapTotal(res.data, list.length);
  }

  async function fetchAssignableUsers() {
    const res = await api.get<unknown>('/api/v2/auth/users?page=1&page_size=200');
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '加载可分配用户失败'));
      return;
    }
    assignableUsers.value = unwrapListRaw(res.data)
      .map(assignableUserFromApi)
      .filter((u): u is AssignableUser => Boolean(u));
  }

  async function fetchSidebarUser() {
    const res = await api.get<unknown>('/api/v2/auth/me');
    if (!res.ok) {
      sidebarUser.value = { username: '未知用户', initial: 'A', role: '用户', is_admin: false };
      return;
    }
    const me = unwrap<{ username?: string; display_name?: string; is_admin?: boolean; role?: string }>(res.data);
    if (!me) {
      sidebarUser.value = { username: '未知用户', initial: 'A', role: '用户', is_admin: false };
      return;
    }
    const name = me.display_name || me.username || '当前用户';
    sidebarUser.value = {
      username: name,
      initial: (name.charAt(0) || 'A').toUpperCase(),
      role: me.is_admin ? '管理员' : (me.role || '用户'),
      is_admin: Boolean(me.is_admin),
    };
  }

  // ------------- Team CRUD ---------------------------------

  async function createTeam(form: TeamFormData): Promise<boolean> {
    if (!form.name.trim()) {
      toast.showToast('warning', '请填写班组名称');
      return false;
    }
    if (!form.department_id.trim()) {
      toast.showToast('warning', '请选择所属科室');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/teams', teamToCreateApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建班组失败'));
        return false;
      }
      toast.showToast('success', '班组创建成功');
      await fetchTeams();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateTeam(id: string, form: TeamFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(`/api/v2/dispatch/teams/${encodeURIComponent(id)}`, teamToUpdateApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新班组失败'));
        return false;
      }
      toast.showToast('success', '班组更新成功');
      await fetchTeams();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deleteTeam(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/teams/${encodeURIComponent(id)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '删除班组失败'));
        return false;
      }
      toast.showToast('success', '班组已删除');
      await fetchTeams();
      return true;
    } catch (err) {
      toast.showToast('error', err instanceof Error ? err.message : '删除班组失败');
      return false;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Equipment CRUD ----------------------------

  async function createEquipment(form: EquipmentFormData): Promise<boolean> {
    if (!form.code.trim()) {
      toast.showToast('warning', '请填写设备代码');
      return false;
    }
    if (!form.department_id.trim()) {
      toast.showToast('warning', '请选择所属科室');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/equipment', equipmentToCreateApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建设备失败'));
        return false;
      }
      toast.showToast('success', '设备创建成功');
      await fetchEquipment();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateEquipment(id: string, form: EquipmentFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/dispatch/equipment/${encodeURIComponent(id)}`,
        equipmentToUpdateApi(form),
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新设备失败'));
        return false;
      }
      toast.showToast('success', '设备更新成功');
      await fetchEquipment();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deleteEquipment(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/equipment/${encodeURIComponent(id)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '删除设备失败'));
        return false;
      }
      toast.showToast('success', '设备已删除');
      await fetchEquipment();
      return true;
    } catch (err) {
      toast.showToast('error', err instanceof Error ? err.message : '删除设备失败');
      return false;
    } finally {
      saving.value = false;
    }
  }

  // Equipment status: matches legacy `PUT /equipment/{id}/status?status=...`.
  async function updateEquipmentStatus(id: string, form: EquipmentStatusFormData): Promise<boolean> {
    saving.value = true;
    try {
      const url = `/api/v2/dispatch/equipment/${encodeURIComponent(id)}/status?status=${encodeURIComponent(form.status)}`;
      const res = await api.put<unknown>(url, undefined);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新设备状态失败'));
        return false;
      }
      const meta: Record<string, string> = {};
      if (form.next_maintenance_date.trim()) meta.next_maintenance_date = form.next_maintenance_date.trim();
      if (Object.keys(meta).length > 0) {
        const metaRes = await api.put<unknown>(`/api/v2/dispatch/equipment/${encodeURIComponent(id)}`, meta);
        if (!metaRes.ok) {
          toast.showToast('error', await extractErrorMessage(metaRes.response, '设备状态已更新，但附加信息保存失败'));
          return false;
        }
      }
      toast.showToast('success', '设备状态已更新');
      await fetchEquipment();
      return true;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Team Type CRUD ----------------------------

  async function createTeamType(form: TeamTypeFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/team-types', teamTypeToApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建班组类型失败'));
        return false;
      }
      toast.showToast('success', '班组类型已创建');
      await fetchTeamTypes();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateTeamType(id: string, form: TeamTypeFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/dispatch/team-types/${encodeURIComponent(id)}`,
        teamTypeToApi(form),
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新班组类型失败'));
        return false;
      }
      toast.showToast('success', '班组类型已更新');
      await fetchTeamTypes();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deleteTeamType(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/team-types/${encodeURIComponent(id)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '删除班组类型失败'));
        return false;
      }
      toast.showToast('success', '班组类型已删除');
      await fetchTeamTypes();
      return true;
    } catch (err) {
      toast.showToast('error', err instanceof Error ? err.message : '删除班组类型失败');
      return false;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Equipment Type CRUD -----------------------

  async function createEquipmentType(form: EquipmentTypeFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/equipment-types', equipmentTypeToApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建设备类型失败'));
        return false;
      }
      toast.showToast('success', '设备类型已创建');
      await fetchEquipmentTypes();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateEquipmentType(id: string, form: EquipmentTypeFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/dispatch/equipment-types/${encodeURIComponent(id)}`,
        equipmentTypeToApi(form),
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新设备类型失败'));
        return false;
      }
      toast.showToast('success', '设备类型已更新');
      await fetchEquipmentTypes();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deleteEquipmentType(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/equipment-types/${encodeURIComponent(id)}`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '删除设备类型失败'));
        return false;
      }
      toast.showToast('success', '设备类型已删除');
      await fetchEquipmentTypes();
      return true;
    } catch (err) {
      toast.showToast('error', err instanceof Error ? err.message : '删除设备类型失败');
      return false;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Department CRUD ---------------------------

  async function createDepartment(form: DepartmentFormData): Promise<boolean> {
    if (!form.name.trim()) {
      toast.showToast('warning', '请填写科室名称');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/resources/departments', departmentToApi(form));
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建科室失败'));
        return false;
      }
      toast.showToast('success', '科室已创建');
      await fetchDepartments(true);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateDepartment(id: string, form: DepartmentFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/dispatch/resources/departments/${encodeURIComponent(id)}`,
        departmentToApi(form),
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新科室失败'));
        return false;
      }
      toast.showToast('success', '科室已更新');
      await fetchDepartments(true);
      return true;
    } finally {
      saving.value = false;
    }
  }

  /** 科室无 DELETE：停用/启用走 PUT is_active。 */
  async function setDepartmentActive(id: string, active: boolean): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/dispatch/resources/departments/${encodeURIComponent(id)}`,
        { is_active: active },
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, active ? '启用科室失败' : '停用科室失败'));
        return false;
      }
      toast.showToast('success', active ? '科室已启用' : '科室已停用');
      await fetchDepartments(true);
      return true;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Team Members ------------------------------

  async function loadTeamMembers(teamId: string) {
    teamMembersLoading.value = true;
    try {
      const res = await api.get<unknown>(`/api/v2/dispatch/teams/${encodeURIComponent(teamId)}/members`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '加载成员失败'));
        return;
      }
      teamMembers.value = unwrapListRaw(res.data)
        .map(teamMemberFromApi)
        .filter((m): m is TeamMember => Boolean(m));
    } finally {
      teamMembersLoading.value = false;
    }
  }

  async function addTeamMember(teamId: string): Promise<boolean> {
    if (!teamMemberAdd.value.user_id) return false;
    teamMemberAddBusy.value = true;
    try {
      const res = await api.post<unknown>(`/api/v2/dispatch/teams/${encodeURIComponent(teamId)}/members`, {
        user_id: teamMemberAdd.value.user_id,
        role: teamMemberAdd.value.role || 'member',
        can_drive: teamMemberAdd.value.can_drive,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '添加成员失败'));
        return false;
      }
      toast.showToast('success', '成员已添加');
      teamMemberAdd.value = { user_id: '', role: 'member', can_drive: false };
      await loadTeamMembers(teamId);
      await fetchTeams();
      return true;
    } finally {
      teamMemberAddBusy.value = false;
    }
  }

  async function removeTeamMember(teamId: string, userId: string): Promise<boolean> {
    const res = await api.delete<unknown>(
      `/api/v2/dispatch/teams/${encodeURIComponent(teamId)}/members/${encodeURIComponent(userId)}`,
    );
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '移除成员失败'));
      return false;
    }
    toast.showToast('success', '成员已移除');
    await loadTeamMembers(teamId);
    await fetchTeams();
    return true;
  }

  // ------------- Modal openers (typed) ---------------------

  function openTeamModal(item?: Team) {
    teamForm.value = item
      ? {
          name: item.name || '',
          code: item.code || '',
          department_id: item.department_id || '',
          leader_id: item.leader_id || '',
          current_status: item.current_status || 'available',
        }
      : { name: '', code: '', department_id: '', leader_id: '', current_status: 'available' };
    modal.value = { kind: 'team', item };
    if (options.loadAssignableUsers !== false && assignableUsers.value.length === 0) {
      void fetchAssignableUsers();
    }
  }

  function openEquipmentModal(item?: Equipment) {
    equipmentForm.value = item
      ? {
          code: item.code || '',
          name: item.name || '',
          equipment_type_id: item.equipment_type_id || '',
          department_id: item.department_id || '',
          license_plate: item.license_plate || '',
          status: item.status || 'available',
          next_maintenance_date: item.next_maintenance_date || '',
        }
      : {
          code: '',
          name: '',
          equipment_type_id: '',
          department_id: '',
          license_plate: '',
          status: 'available',
          next_maintenance_date: '',
        };
    modal.value = { kind: 'equipment', item };
  }

  function openDepartmentModal(item?: Department) {
    departmentForm.value = item
      ? {
          name: item.name || '',
          code: item.code || '',
          description: item.description || '',
          manager_id: item.manager_id || '',
        }
      : { name: '', code: '', description: '', manager_id: '' };
    modal.value = { kind: 'department', item };
    if (options.loadAssignableUsers !== false && assignableUsers.value.length === 0) {
      void fetchAssignableUsers();
    }
  }

  function openTeamTypeModal(item?: TeamType) {
    teamTypeForm.value = item
      ? {
          name: item.name || '',
          code: item.code || '',
          department_id: item.department_id || '',
          color: item.color || '',
          is_driver_type: Boolean(item.is_driver_type),
          task_types: (item.task_types ?? []).join(', '),
          description: item.description || '',
        }
      : {
          name: '',
          code: '',
          department_id: '',
          color: '',
          is_driver_type: false,
          task_types: '',
          description: '',
        };
    modal.value = { kind: 'team-type', item };
  }

  function openEquipmentTypeModal(item?: EquipmentType) {
    equipmentTypeForm.value = item
      ? {
          name: item.name || '',
          code: item.code || '',
          category: item.category || '',
          requires_driver: Boolean(item.requires_driver),
          icon: item.icon || '',
          description: item.description || '',
        }
      : {
          name: '',
          code: '',
          category: '',
          requires_driver: false,
          icon: '',
          description: '',
        };
    modal.value = { kind: 'equipment-type', item };
  }

  function openEquipmentStatusModal(item: Equipment) {
    equipmentStatusForm.value = {
      status: item.status || 'available',
      next_maintenance_date: item.next_maintenance_date || '',
    };
    modal.value = { kind: 'equipment-status', item };
  }

  async function openTeamMembersDrawer(team: Team) {
    modal.value = { kind: 'team-members', team };
    teamMemberAdd.value = { user_id: '', role: 'member', can_drive: false };
    memberSearch.value = '';
    const tasks: Promise<unknown>[] = [loadTeamMembers(team.id)];
    if (options.loadAssignableUsers !== false) {
      tasks.push(fetchAssignableUsers());
    }
    await Promise.all(tasks);
  }

  function closeModal() {
    modal.value = { kind: 'none' };
  }

  // ------------- Save dispatcher ---------------------------

  async function saveCurrentModal(): Promise<void> {
    const m = modal.value;
    let ok = false;
    if (m.kind === 'team') {
      ok = m.item ? await updateTeam(m.item.id, teamForm.value) : await createTeam(teamForm.value);
    } else if (m.kind === 'equipment') {
      ok = m.item
        ? await updateEquipment(m.item.id, equipmentForm.value)
        : await createEquipment(equipmentForm.value);
    } else if (m.kind === 'team-type') {
      ok = m.item ? await updateTeamType(m.item.id, teamTypeForm.value) : await createTeamType(teamTypeForm.value);
    } else if (m.kind === 'equipment-type') {
      ok = m.item
        ? await updateEquipmentType(m.item.id, equipmentTypeForm.value)
        : await createEquipmentType(equipmentTypeForm.value);
    } else if (m.kind === 'equipment-status') {
      ok = await updateEquipmentStatus(m.item.id, equipmentStatusForm.value);
    } else if (m.kind === 'department') {
      ok = m.item
        ? await updateDepartment(m.item.id, departmentForm.value)
        : await createDepartment(departmentForm.value);
    }
    if (ok) closeModal();
  }

  // ------------- Section switching -------------------------

  function switchSection(section: ResourceSection) {
    activeSection.value = section;
    if (section === 'teams') fetchTeams();
    else if (section === 'equipment') fetchEquipment();
    else if (section === 'team-types') fetchTeamTypes();
    else if (section === 'equipment-types') fetchEquipmentTypes();
    else if (section === 'departments' || section === 'qualifications') fetchDepartments(true);
    // 'terminals' 板块由 useTerminalDirectory 自行加载
  }

  onMounted(() => {
    fetchTeams();
    fetchTeamTypes();
    fetchEquipmentTypes();
    fetchDepartments();
    if (options.loadAssignableUsers !== false) {
      fetchAssignableUsers();
    }
    fetchSidebarUser();
  });

  return {
    // section / loading
    activeSection,
    loading,
    saving,

    // data
    teams: filteredTeams,
    rawTeams: teams,
    equipment: filteredEquipment,
    rawEquipment: equipment,
    teamTypes: filteredTeamTypes,
    rawTeamTypes: teamTypes,
    equipmentTypes: filteredEquipmentTypes,
    rawEquipmentTypes: equipmentTypes,
    departments: filteredDepartments,
    rawDepartments: departments,
    assignableUsers,
    filteredAssignableUsers,

    // totals
    teamsTotal,
    equipmentTotal,
    teamTypesTotal,
    equipmentTypesTotal,
    departmentsTotal,

    // filters
    teamSearch,
    teamTypeFilter,
    teamStatusFilter,
    equipmentSearch,
    equipmentTypeFilter,
    equipmentStatusFilter,
    teamTypeSearch,
    equipmentTypeSearch,
    departmentSearch,
    memberSearch,

    // modal
    modal,
    teamForm,
    equipmentForm,
    teamTypeForm,
    equipmentTypeForm,
    equipmentStatusForm,
    departmentForm,
    openTeamModal,
    openEquipmentModal,
    openTeamTypeModal,
    openEquipmentTypeModal,
    openEquipmentStatusModal,
    openDepartmentModal,
    openTeamMembersDrawer,
    closeModal,
    saveCurrentModal,

    // team members
    teamMembers,
    teamMembersLoading,
    teamMemberAdd,
    teamMemberAddBusy,
    addTeamMember,
    removeTeamMember,
    loadTeamMembers,

    // CRUD direct
    createTeam,
    updateTeam,
    deleteTeam,
    createEquipment,
    updateEquipment,
    deleteEquipment,
    updateEquipmentStatus,
    createTeamType,
    updateTeamType,
    deleteTeamType,
    createEquipmentType,
    updateEquipmentType,
    deleteEquipmentType,
    createDepartment,
    updateDepartment,
    setDepartmentActive,

    // fetchers
    fetchTeams,
    fetchEquipment,
    fetchTeamTypes,
    fetchEquipmentTypes,
    fetchDepartments,
    fetchAssignableUsers,
    fetchSidebarUser,

    // section
    switchSection,

    // sidebar
    sidebarUser,
  };
}
