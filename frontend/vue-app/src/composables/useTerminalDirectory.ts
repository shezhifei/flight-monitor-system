import { computed, ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

// ----------------------------------------------------------------------------
// 空间目录（航站楼/登机口/行李转盘/机位挂楼）—— 对应后端
// dispatch_resources/mod.rs 的 configure_terminal_directory_routes。
// 字段名为后端 serde 输出（terminal_id / gate_id / carousel_id）。
// ----------------------------------------------------------------------------

export interface Terminal {
  terminal_id: string;
  code: string;
  name: string;
  is_active: boolean;
}

export interface Gate {
  gate_id: string;
  code: string;
  name?: string | null;
  is_active: boolean;
}

export interface BaggageCarousel {
  carousel_id: string;
  code: string;
  name?: string | null;
  is_active: boolean;
}

export interface Stand {
  id: string;
  code: string;
  name?: string | null;
  terminal?: string | null;
  area?: string | null;
  stand_type?: string | null;
  size_category?: string | null;
  position_lat?: number;
  position_lng?: number;
  is_active?: boolean;
}

export interface TerminalDirectory {
  terminal: Terminal;
  stands: Stand[];
  gates: Gate[];
  carousels: BaggageCarousel[];
}

export type DirectoryModal =
  | { kind: 'none' }
  | { kind: 'terminal'; item?: Terminal }
  | { kind: 'gate'; item?: Gate }
  | { kind: 'carousel'; item?: BaggageCarousel }
  | { kind: 'stand'; item?: Stand }
  | { kind: 'attach-stand' }
  | { kind: 'conflict'; title: string; message: string; details: unknown[] };

export interface TerminalFormData {
  code: string;
  name: string;
}

export interface GateFormData {
  code: string;
  name: string;
}

export interface CarouselFormData {
  code: string;
  name: string;
}

export interface StandFormData {
  code: string;
  name: string;
  area: string;
  stand_type: string;
  size_category: string;
}

// ----------------------------------------------------------------------------
// helpers（与 useResourceManager 同款 unwrap 约定：响应可能是裸体或 { success, data }）
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

function asBool(value: unknown): boolean {
  return Boolean(value);
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

/**
 * 后端 409 的占用明细内联在 message 里：
 *   "停用楼失败：存在未结束占用/分配; 明细: [{...}]"
 * 拆出基础文案与明细数组；解析失败时明细为空、保留原文。
 */
export function splitConflictMessage(message: string): { base: string; details: unknown[] } {
  const marker = '明细:';
  const idx = message.indexOf(marker);
  if (idx < 0) return { base: message, details: [] };
  const base = message
    .slice(0, idx)
    .replace(/[;；\s]+$/, '');
  const raw = message.slice(idx + marker.length).trim();
  try {
    const parsed: unknown = JSON.parse(raw);
    return { base, details: Array.isArray(parsed) ? parsed : [parsed] };
  } catch {
    return { base: message, details: [] };
  }
}

export function terminalFromApi(raw: unknown): Terminal | null {
  const r = asRecord(raw);
  if (!r) return null;
  const terminal_id = asString(r.terminal_id);
  const code = asString(r.code);
  const name = asString(r.name);
  if (!terminal_id || !code || !name) return null;
  return { terminal_id, code, name, is_active: r.is_active === undefined ? true : asBool(r.is_active) };
}

export function gateFromApi(raw: unknown): Gate | null {
  const r = asRecord(raw);
  if (!r) return null;
  const gate_id = asString(r.gate_id);
  const code = asString(r.code);
  if (!gate_id || !code) return null;
  return {
    gate_id,
    code,
    name: asString(r.name),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function carouselFromApi(raw: unknown): BaggageCarousel | null {
  const r = asRecord(raw);
  if (!r) return null;
  const carousel_id = asString(r.carousel_id);
  const code = asString(r.code);
  if (!carousel_id || !code) return null;
  return {
    carousel_id,
    code,
    name: asString(r.name),
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function standFromApi(raw: unknown): Stand | null {
  const r = asRecord(raw);
  if (!r) return null;
  const id = asString(r.id);
  const code = asString(r.code);
  if (!id || !code) return null;
  return {
    id,
    code,
    name: asString(r.name),
    terminal: asString(r.terminal),
    area: asString(r.area),
    stand_type: asString(r.stand_type),
    size_category: asString(r.size_category),
    position_lat: typeof r.position_lat === 'number' ? r.position_lat : undefined,
    position_lng: typeof r.position_lng === 'number' ? r.position_lng : undefined,
    is_active: r.is_active === undefined ? true : asBool(r.is_active),
  };
}

export function directoryFromApi(raw: unknown): TerminalDirectory | null {
  const r = asRecord(raw);
  if (!r) return null;
  const terminal = terminalFromApi(r.terminal);
  if (!terminal) return null;
  return {
    terminal,
    stands: (Array.isArray(r.stands) ? r.stands : []).map(standFromApi).filter((s): s is Stand => Boolean(s)),
    gates: (Array.isArray(r.gates) ? r.gates : []).map(gateFromApi).filter((g): g is Gate => Boolean(g)),
    carousels: (Array.isArray(r.carousels) ? r.carousels : [])
      .map(carouselFromApi)
      .filter((c): c is BaggageCarousel => Boolean(c)),
  };
}

// ----------------------------------------------------------------------------
// Composable
// ----------------------------------------------------------------------------

export function useTerminalDirectory() {
  const api = useApi();
  const toast = useToast();

  const terminals = ref<Terminal[]>([]);
  const loading = ref(false);
  const saving = ref(false);
  const terminalSearch = ref('');

  const selectedTerminalId = ref('');
  const directory = ref<TerminalDirectory | null>(null);
  const contextLoading = ref(false);

  const allStands = ref<Stand[]>([]);
  const attachStandId = ref('');

  const modal = ref<DirectoryModal>({ kind: 'none' });
  const terminalForm = ref<TerminalFormData>({ code: '', name: '' });
  const gateForm = ref<GateFormData>({ code: '', name: '' });
  const carouselForm = ref<CarouselFormData>({ code: '', name: '' });
  const standForm = ref<StandFormData>({
    code: '',
    name: '',
    area: '',
    stand_type: '',
    size_category: '',
  });

  const filteredTerminals = computed(() => {
    const q = terminalSearch.value.trim().toLowerCase();
    if (!q) return terminals.value;
    return terminals.value.filter(t => `${t.code} ${t.name}`.toLowerCase().includes(q));
  });

  const selectedTerminal = computed(() => directory.value?.terminal ?? null);

  /** 可挂载的机位 = 全量启用机位中尚未挂到当前楼的 */
  const attachableStands = computed(() => {
    const attached = new Set((directory.value?.stands ?? []).map(s => s.id));
    return allStands.value.filter(s => !attached.has(s.id));
  });

  // ------------- fetchers ----------------------------------

  async function fetchTerminals() {
    loading.value = true;
    try {
      const res = await api.get<unknown>('/api/v2/dispatch/terminals?include_inactive=true');
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '加载航站楼失败'));
        return;
      }
      terminals.value = unwrapListRaw(res.data)
        .map(terminalFromApi)
        .filter((t): t is Terminal => Boolean(t));
    } finally {
      loading.value = false;
    }
  }

  async function fetchContext(terminalId: string) {
    contextLoading.value = true;
    try {
      const res = await api.get<unknown>(`/api/v2/dispatch/terminals/${encodeURIComponent(terminalId)}/context`);
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '加载航站楼上下文失败'));
        return;
      }
      directory.value = directoryFromApi(unwrap(res.data));
    } finally {
      contextLoading.value = false;
    }
  }

  async function fetchAllStands() {
    const res = await api.get<unknown>('/api/v2/dispatch/resources/stands?include_inactive=false&page=1&page_size=500');
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '加载机位列表失败'));
      return;
    }
    allStands.value = unwrapListRaw(res.data)
      .map(standFromApi)
      .filter((s): s is Stand => Boolean(s));
  }

  async function selectTerminal(terminalId: string) {
    selectedTerminalId.value = terminalId;
    await fetchContext(terminalId);
  }

  // ------------- 409 冲突明细 --------------------------------

  /** 停楼/移出成员遇 409：弹出占用明细模态；其它错误走 toast。返回是否已按冲突处理。 */
  async function handleWriteError(response: Response, fallback: string, conflictTitle: string): Promise<void> {
    const message = await extractErrorMessage(response, fallback);
    if (response.status === 409) {
      const { base, details } = splitConflictMessage(message);
      modal.value = { kind: 'conflict', title: conflictTitle, message: base, details };
      return;
    }
    toast.showToast('error', message);
  }

  // ------------- Terminal CRUD -----------------------------

  async function createTerminal(form: TerminalFormData): Promise<boolean> {
    if (!form.code.trim() || !form.name.trim()) {
      toast.showToast('warning', '请填写航站楼代码与名称');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/terminals', {
        code: form.code.trim(),
        name: form.name.trim(),
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建航站楼失败'));
        return false;
      }
      toast.showToast('success', '航站楼已创建');
      await fetchTerminals();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateTerminal(id: string, form: TerminalFormData): Promise<boolean> {
    if (!form.name.trim()) {
      toast.showToast('warning', '请填写航站楼名称');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.patch<unknown>(`/api/v2/dispatch/terminals/${encodeURIComponent(id)}`, {
        name: form.name.trim(),
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新航站楼失败'));
        return false;
      }
      toast.showToast('success', '航站楼已更新');
      await fetchTerminals();
      if (selectedTerminalId.value === id) await fetchContext(id);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deactivateTerminal(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>(`/api/v2/dispatch/terminals/${encodeURIComponent(id)}/deactivate`, undefined);
      if (!res.ok) {
        await handleWriteError(res.response, '停用航站楼失败', '停用航站楼被占用阻止');
        return false;
      }
      toast.showToast('success', '航站楼已停用');
      await fetchTerminals();
      if (selectedTerminalId.value === id) await fetchContext(id);
      return true;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Gate CRUD ---------------------------------

  async function createGate(form: GateFormData): Promise<boolean> {
    const terminalId = selectedTerminalId.value;
    if (!terminalId) {
      toast.showToast('warning', '请先选择航站楼');
      return false;
    }
    if (!form.code.trim()) {
      toast.showToast('warning', '请填写登机口代码');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/gates', {
        terminal_id: terminalId,
        code: form.code.trim(),
        name: form.name.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建登机口失败'));
        return false;
      }
      toast.showToast('success', '登机口已创建');
      await fetchContext(terminalId);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateGate(id: string, form: GateFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.patch<unknown>(`/api/v2/dispatch/gates/${encodeURIComponent(id)}`, {
        name: form.name.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新登机口失败'));
        return false;
      }
      toast.showToast('success', '登机口已更新');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deactivateGate(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>(`/api/v2/dispatch/gates/${encodeURIComponent(id)}/deactivate`, undefined);
      if (!res.ok) {
        await handleWriteError(res.response, '停用登机口失败', '停用登机口被占用阻止');
        return false;
      }
      toast.showToast('success', '登机口已停用');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  /** 重新启用登机口（PATCH is_active）。 */
  async function reactivateGate(id: string): Promise<boolean> {
    const res = await api.patch<unknown>(`/api/v2/dispatch/gates/${encodeURIComponent(id)}`, { is_active: true });
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '启用登机口失败'));
      return false;
    }
    toast.showToast('success', '登机口已启用');
    if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
    return true;
  }

  // ------------- Carousel CRUD -----------------------------

  async function createCarousel(form: CarouselFormData): Promise<boolean> {
    const terminalId = selectedTerminalId.value;
    if (!terminalId) {
      toast.showToast('warning', '请先选择航站楼');
      return false;
    }
    if (!form.code.trim()) {
      toast.showToast('warning', '请填写转盘代码');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/carousels', {
        terminal_id: terminalId,
        code: form.code.trim(),
        name: form.name.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建行李转盘失败'));
        return false;
      }
      toast.showToast('success', '行李转盘已创建');
      await fetchContext(terminalId);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateCarousel(id: string, form: CarouselFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.patch<unknown>(`/api/v2/dispatch/carousels/${encodeURIComponent(id)}`, {
        name: form.name.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新行李转盘失败'));
        return false;
      }
      toast.showToast('success', '行李转盘已更新');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deactivateCarousel(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>(
        `/api/v2/dispatch/carousels/${encodeURIComponent(id)}/deactivate`,
        undefined,
      );
      if (!res.ok) {
        await handleWriteError(res.response, '停用行李转盘失败', '停用行李转盘被占用阻止');
        return false;
      }
      toast.showToast('success', '行李转盘已停用');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function reactivateCarousel(id: string): Promise<boolean> {
    const res = await api.patch<unknown>(`/api/v2/dispatch/carousels/${encodeURIComponent(id)}`, { is_active: true });
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '启用行李转盘失败'));
      return false;
    }
    toast.showToast('success', '行李转盘已启用');
    if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
    return true;
  }

  // ------------- Stand CRUD --------------------------------

  async function createStand(form: StandFormData): Promise<boolean> {
    const terminalId = selectedTerminalId.value;
    if (!terminalId) {
      toast.showToast('warning', '请先选择航站楼');
      return false;
    }
    if (!form.code.trim()) {
      toast.showToast('warning', '请填写机位代码');
      return false;
    }
    saving.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/dispatch/stands', {
        terminal_id: terminalId,
        code: form.code.trim(),
        name: form.name.trim() || null,
        area: form.area.trim() || null,
        stand_type: form.stand_type.trim() || null,
        size_category: form.size_category.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '创建机位失败'));
        return false;
      }
      toast.showToast('success', '机位已创建');
      await fetchContext(terminalId);
      await fetchAllStands();
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function updateStand(id: string, form: StandFormData): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.patch<unknown>(`/api/v2/dispatch/stands/${encodeURIComponent(id)}`, {
        name: form.name.trim() || null,
        area: form.area.trim() || null,
        stand_type: form.stand_type.trim() || null,
        size_category: form.size_category.trim() || null,
      });
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '更新机位失败'));
        return false;
      }
      toast.showToast('success', '机位已更新');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function deactivateStand(id: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.post<unknown>(`/api/v2/dispatch/stands/${encodeURIComponent(id)}/deactivate`, undefined);
      if (!res.ok) {
        await handleWriteError(res.response, '停用机位失败', '停用机位被占用阻止');
        return false;
      }
      toast.showToast('success', '机位已停用');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function reactivateStand(id: string): Promise<boolean> {
    const res = await api.patch<unknown>(`/api/v2/dispatch/stands/${encodeURIComponent(id)}`, { is_active: true });
    if (!res.ok) {
      toast.showToast('error', await extractErrorMessage(res.response, '启用机位失败'));
      return false;
    }
    toast.showToast('success', '机位已启用');
    if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
    return true;
  }

  // ------------- 成员关系（挂楼/移出） -----------------------

  async function attachStand(): Promise<boolean> {
    const terminalId = selectedTerminalId.value;
    const standId = attachStandId.value;
    if (!terminalId || !standId) return false;
    saving.value = true;
    try {
      const res = await api.post<unknown>(
        `/api/v2/dispatch/terminals/${encodeURIComponent(terminalId)}/stands/${encodeURIComponent(standId)}`,
        undefined,
      );
      if (!res.ok) {
        toast.showToast('error', await extractErrorMessage(res.response, '挂载机位失败'));
        return false;
      }
      toast.showToast('success', '机位已挂载');
      attachStandId.value = '';
      await fetchContext(terminalId);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function detachStand(standId: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/terminals/stands/${encodeURIComponent(standId)}`);
      if (!res.ok) {
        await handleWriteError(res.response, '移出机位失败', '移出机位被占用阻止');
        return false;
      }
      toast.showToast('success', '机位已移出');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function detachGate(gateId: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/terminals/gates/${encodeURIComponent(gateId)}`);
      if (!res.ok) {
        await handleWriteError(res.response, '移出登机口失败', '移出登机口被占用阻止');
        return false;
      }
      toast.showToast('success', '登机口已移出航站楼');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  async function detachCarousel(carouselId: string): Promise<boolean> {
    saving.value = true;
    try {
      const res = await api.delete<unknown>(`/api/v2/dispatch/terminals/carousels/${encodeURIComponent(carouselId)}`);
      if (!res.ok) {
        await handleWriteError(res.response, '移出行李转盘失败', '移出行李转盘被占用阻止');
        return false;
      }
      toast.showToast('success', '行李转盘已移出航站楼');
      if (selectedTerminalId.value) await fetchContext(selectedTerminalId.value);
      return true;
    } finally {
      saving.value = false;
    }
  }

  // ------------- Modal openers -----------------------------

  function openTerminalModal(item?: Terminal) {
    terminalForm.value = item ? { code: item.code, name: item.name } : { code: '', name: '' };
    modal.value = { kind: 'terminal', item };
  }

  function openGateModal(item?: Gate) {
    gateForm.value = item ? { code: item.code, name: item.name || '' } : { code: '', name: '' };
    modal.value = { kind: 'gate', item };
  }

  function openCarouselModal(item?: BaggageCarousel) {
    carouselForm.value = item ? { code: item.code, name: item.name || '' } : { code: '', name: '' };
    modal.value = { kind: 'carousel', item };
  }

  function openStandModal(item?: Stand) {
    standForm.value = item
      ? {
          code: item.code,
          name: item.name || '',
          area: item.area || '',
          stand_type: item.stand_type || '',
          size_category: item.size_category || '',
        }
      : { code: '', name: '', area: '', stand_type: '', size_category: '' };
    modal.value = { kind: 'stand', item };
  }

  function openAttachStandModal() {
    attachStandId.value = '';
    modal.value = { kind: 'attach-stand' };
    if (allStands.value.length === 0) void fetchAllStands();
  }

  function closeModal() {
    modal.value = { kind: 'none' };
  }

  async function saveCurrentModal(): Promise<void> {
    const m = modal.value;
    let ok = false;
    if (m.kind === 'terminal') {
      ok = m.item
        ? await updateTerminal(m.item.terminal_id, terminalForm.value)
        : await createTerminal(terminalForm.value);
    } else if (m.kind === 'gate') {
      ok = m.item ? await updateGate(m.item.gate_id, gateForm.value) : await createGate(gateForm.value);
    } else if (m.kind === 'carousel') {
      ok = m.item
        ? await updateCarousel(m.item.carousel_id, carouselForm.value)
        : await createCarousel(carouselForm.value);
    } else if (m.kind === 'stand') {
      ok = m.item ? await updateStand(m.item.id, standForm.value) : await createStand(standForm.value);
    } else if (m.kind === 'attach-stand') {
      ok = await attachStand();
    }
    if (ok) closeModal();
  }

  return {
    // data
    terminals: filteredTerminals,
    selectedTerminalId,
    selectedTerminal,
    directory,
    allStands,
    attachableStands,
    attachStandId,
    terminalSearch,

    // loading
    loading,
    saving,
    contextLoading,

    // modal
    modal,
    terminalForm,
    gateForm,
    carouselForm,
    standForm,
    openTerminalModal,
    openGateModal,
    openCarouselModal,
    openStandModal,
    openAttachStandModal,
    closeModal,
    saveCurrentModal,

    // actions
    fetchTerminals,
    fetchContext,
    fetchAllStands,
    selectTerminal,
    createTerminal,
    updateTerminal,
    deactivateTerminal,
    createGate,
    updateGate,
    deactivateGate,
    reactivateGate,
    createCarousel,
    updateCarousel,
    deactivateCarousel,
    reactivateCarousel,
    createStand,
    updateStand,
    deactivateStand,
    reactivateStand,
    attachStand,
    detachStand,
    detachGate,
    detachCarousel,
  };
}
