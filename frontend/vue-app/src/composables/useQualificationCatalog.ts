import { computed, ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';
import type { Department } from './useResourceManager';

export interface QualificationCatalog {
  id: string;
  department_id: string;
  qualification_code: string;
  qualification_name: string;
  description?: string | null;
  is_active: boolean;
}

export interface QualificationLevel {
  id: string;
  department_id: string;
  qualification_code: string;
  level_code: string;
  level_name: string;
  level_rank: number;
  is_active: boolean;
}

export interface QualificationFormData {
  qualification_code: string;
  qualification_name: string;
  description: string;
}

export interface QualificationLevelFormData {
  level_code: string;
  level_name: string;
  level_rank: number;
}

export type QualificationModal =
  | { kind: 'none' }
  | { kind: 'qualification'; item?: QualificationCatalog }
  | { kind: 'level'; qualification: QualificationCatalog };

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : null;
}

function asString(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return null;
}

function unwrap<T>(payload: unknown): T | null {
  if (payload && typeof payload === 'object' && 'data' in (payload as Record<string, unknown>)) {
    return ((payload as Record<string, unknown>).data ?? null) as T | null;
  }
  return (payload ?? null) as T | null;
}

function unwrapListRaw(payload: unknown): unknown[] {
  const data = unwrap<unknown[] | { items?: unknown[] }>(payload);
  if (Array.isArray(data)) return data;
  if (data && typeof data === 'object' && Array.isArray((data as { items?: unknown[] }).items)) {
    return (data as { items: unknown[] }).items;
  }
  return [];
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

function catalogFromApi(raw: unknown): QualificationCatalog | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const department_id = asString(r.department_id);
  const qualification_code = asString(r.qualification_code);
  const qualification_name = asString(r.qualification_name);
  if (!id || !department_id || !qualification_code || !qualification_name) return null;
  return {
    id,
    department_id,
    qualification_code,
    qualification_name,
    description: asString(r.description),
    is_active: r.is_active !== false,
  };
}

function levelFromApi(raw: unknown): QualificationLevel | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const department_id = asString(r.department_id);
  const qualification_code = asString(r.qualification_code);
  const level_code = asString(r.level_code);
  const level_name = asString(r.level_name);
  if (!id || !department_id || !qualification_code || !level_code || !level_name) return null;
  return {
    id,
    department_id,
    qualification_code,
    level_name,
    level_code,
    level_rank: typeof r.level_rank === 'number' ? r.level_rank : Number(r.level_rank) || 1,
    is_active: r.is_active !== false,
  };
}

function emptyForm(): QualificationFormData {
  return { qualification_code: '', qualification_name: '', description: '' };
}

function emptyLevelForm(): QualificationLevelFormData {
  return { level_code: '', level_name: '', level_rank: 1 };
}

function ruleBase(departmentId: string): string {
  return `/api/v2/dispatch/rules/departments/${encodeURIComponent(departmentId)}`;
}

export function useQualificationCatalog() {
  const api = useApi();
  const toast = useToast();

  const selectedDepartmentId = ref('');
  const catalogs = ref<QualificationCatalog[]>([]);
  const levels = ref<QualificationLevel[]>([]);
  const search = ref('');
  const loading = ref(false);
  const saving = ref(false);
  const modal = ref<QualificationModal>({ kind: 'none' });
  const form = ref<QualificationFormData>(emptyForm());
  const levelForm = ref<QualificationLevelFormData>(emptyLevelForm());

  const filteredCatalogs = computed(() => {
    const q = search.value.trim().toLowerCase();
    if (!q) return catalogs.value;
    return catalogs.value.filter((item) =>
      `${item.qualification_code} ${item.qualification_name}`.toLowerCase().includes(q),
    );
  });

  function levelsFor(code: string): QualificationLevel[] {
    return levels.value
      .filter((item) => item.qualification_code === code)
      .slice()
      .sort((a, b) => b.level_rank - a.level_rank || a.level_code.localeCompare(b.level_code));
  }

  async function selectDepartment(departmentId: string): Promise<void> {
    selectedDepartmentId.value = departmentId;
    catalogs.value = [];
    levels.value = [];
    if (!departmentId) return;
    await refresh();
  }

  async function refresh(): Promise<void> {
    const departmentId = selectedDepartmentId.value;
    if (!departmentId) return;
    loading.value = true;
    try {
      const [catalogRes, levelRes] = await Promise.all([
        api.get<unknown>(`${ruleBase(departmentId)}/qualifications?include_inactive=true`),
        api.get<unknown>(`${ruleBase(departmentId)}/qualification-levels?include_inactive=true`),
      ]);
      if (!catalogRes.ok) {
        toast.showToast('error', await extractErrorMessage(catalogRes.response, '加载资质目录失败'));
        return;
      }
      if (!levelRes.ok) {
        toast.showToast('error', await extractErrorMessage(levelRes.response, '加载资质等级失败'));
        return;
      }
      catalogs.value = unwrapListRaw(catalogRes.data)
        .map(catalogFromApi)
        .filter((item): item is QualificationCatalog => Boolean(item));
      levels.value = unwrapListRaw(levelRes.data)
        .map(levelFromApi)
        .filter((item): item is QualificationLevel => Boolean(item));
    } finally {
      loading.value = false;
    }
  }

  async function saveCatalog(payload: {
    qualification_code: string;
    qualification_name: string;
    description?: string | null;
    is_active: boolean;
  }): Promise<boolean> {
    const departmentId = selectedDepartmentId.value;
    if (!departmentId) {
      toast.showToast('warning', '请先选择科室');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>(`${ruleBase(departmentId)}/qualifications`, payload);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '保存资质目录失败'));
        return false;
      }
      await refresh();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function createOrUpdateQualification(data: QualificationFormData, existing?: QualificationCatalog): Promise<boolean> {
    if (!data.qualification_code.trim() || !data.qualification_name.trim()) {
      toast.showToast('warning', '请填写资质编码和名称');
      return false;
    }
    const ok = await saveCatalog({
      qualification_code: data.qualification_code.trim(),
      qualification_name: data.qualification_name.trim(),
      description: data.description.trim() || null,
      is_active: existing?.is_active !== false,
    });
    if (ok) toast.showToast('success', existing ? '资质已更新' : '资质已创建');
    return ok;
  }

  async function setCatalogActive(item: QualificationCatalog, active: boolean): Promise<boolean> {
    const ok = await saveCatalog({
      qualification_code: item.qualification_code,
      qualification_name: item.qualification_name,
      description: item.description ?? null,
      is_active: active,
    });
    if (ok) toast.showToast('success', active ? '资质已启用' : '资质已停用');
    return ok;
  }

  async function saveLevel(
    qualification: QualificationCatalog,
    data: QualificationLevelFormData,
    isActive = true,
  ): Promise<boolean> {
    const departmentId = selectedDepartmentId.value;
    if (!departmentId) return false;
    if (!data.level_code.trim() || !data.level_name.trim()) {
      toast.showToast('warning', '请填写等级编码和名称');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>(`${ruleBase(departmentId)}/qualification-levels`, {
        qualification_code: qualification.qualification_code,
        level_code: data.level_code.trim(),
        level_name: data.level_name.trim(),
        level_rank: Number.isFinite(data.level_rank) ? data.level_rank : 1,
        is_active: isActive,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '保存资质等级失败'));
        return false;
      }
      toast.showToast('success', isActive ? '等级已保存' : '等级已停用');
      await refresh();
      return true;
    } finally {
      saving.value = false;
    }
  }

  function openQualificationModal(item?: QualificationCatalog) {
    form.value = item
      ? {
          qualification_code: item.qualification_code,
          qualification_name: item.qualification_name,
          description: item.description || '',
        }
      : emptyForm();
    modal.value = { kind: 'qualification', item };
  }

  function openLevelModal(qualification: QualificationCatalog) {
    levelForm.value = emptyLevelForm();
    modal.value = { kind: 'level', qualification };
  }

  function closeModal() {
    modal.value = { kind: 'none' };
  }

  async function saveCurrentModal(): Promise<void> {
    const m = modal.value;
    let ok = false;
    if (m.kind === 'qualification') {
      ok = await createOrUpdateQualification(form.value, m.item);
    } else if (m.kind === 'level') {
      ok = await saveLevel(m.qualification, levelForm.value);
    }
    if (ok) closeModal();
  }

  function departmentOptions(departments: Department[]): Array<{ value: string; label: string }> {
    return [
      { value: '', label: '请选择科室' },
      ...departments
        .filter((d) => d.is_active !== false)
        .map((d) => ({ value: d.id, label: d.code ? `${d.name}（${d.code}）` : d.name })),
    ];
  }

  return {
    selectedDepartmentId,
    catalogs: filteredCatalogs,
    rawCatalogs: catalogs,
    levels,
    search,
    loading,
    saving,
    modal,
    form,
    levelForm,
    levelsFor,
    selectDepartment,
    refresh,
    setCatalogActive,
    saveLevel,
    openQualificationModal,
    openLevelModal,
    closeModal,
    saveCurrentModal,
    departmentOptions,
  };
}
