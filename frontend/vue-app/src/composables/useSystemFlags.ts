import { ref, computed, onMounted } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

/** Mirrors Rust system flag entries: path, value, type, category, label, description, masked. */
export interface SystemFlag {
  path: string;
  value?: unknown;
  type?: string;
  category?: string;
  label?: string;
  description?: string;
  masked?: boolean;
}

interface ApiErrorPayload {
  message?: string;
  detail?: string;
  error?: string | { message?: string };
  data?: unknown;
}

function extractApiError(data: unknown, fallback: string): string {
  if (data && typeof data === 'object') {
    const payload = data as ApiErrorPayload;
    if (typeof payload.message === 'string' && payload.message.trim()) return payload.message;
    if (typeof payload.detail === 'string' && payload.detail.trim()) return payload.detail;
    if (typeof payload.error === 'string' && payload.error.trim()) return payload.error;
    if (payload.error && typeof payload.error === 'object' && typeof payload.error.message === 'string') {
      return payload.error.message;
    }
  }
  return fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function categoryFromFlag(flag: Pick<SystemFlag, 'category' | 'path'>): string {
  if (typeof flag.category === 'string' && flag.category.trim()) {
    return flag.category;
  }
  const path = typeof flag.path === 'string' ? flag.path : '';
  const head = path.split('.').find((part) => part.length > 0);
  return head || 'general';
}

function normalizeFlag(raw: unknown): SystemFlag | null {
  if (!isRecord(raw)) {
    return null;
  }
  const path = typeof raw.path === 'string' ? raw.path.trim() : '';
  if (!path) {
    return null;
  }
  return {
    path,
    value: raw.value,
    type: typeof raw.type === 'string' ? raw.type : undefined,
    category: categoryFromFlag({
      category: typeof raw.category === 'string' ? raw.category : undefined,
      path,
    }),
    label: typeof raw.label === 'string' ? raw.label : undefined,
    description: typeof raw.description === 'string' ? raw.description : undefined,
    masked: Boolean(raw.masked),
  };
}

function unwrapFlagsPayload(payload: unknown): SystemFlag[] {
  if (Array.isArray(payload)) {
    return payload.map(normalizeFlag).filter((f): f is SystemFlag => f !== null);
  }
  if (!isRecord(payload)) {
    return [];
  }
  const nested = isRecord(payload.data) ? payload.data : payload;
  const list = Array.isArray(nested)
    ? nested
    : Array.isArray(nested.flags)
      ? nested.flags
      : [];
  return list.map(normalizeFlag).filter((f): f is SystemFlag => f !== null);
}

export function useSystemFlags() {
  const api = useApi();
  const toast = useToast();
  const loading = ref(true);
  const flags = ref<SystemFlag[]>([]);
  const categories = ref<string[]>([]);
  /** Legacy sidebar uses a fixed taxonomy; `all` means no category filter. */
  const activeCategory = ref('all');
  const searchQuery = ref('');
  const editingFlag = ref<SystemFlag | null>(null);
  const error = ref('');

  const filteredFlags = computed(() => {
    let result = flags.value;
    if (activeCategory.value && activeCategory.value !== 'all') {
      result = result.filter((f) => categoryFromFlag(f) === activeCategory.value);
    }
    if (searchQuery.value) {
      const q = searchQuery.value.toLowerCase();
      result = result.filter((f) =>
        f.path.toLowerCase().includes(q)
        || (f.label?.toLowerCase().includes(q) ?? false)
        || (f.description?.toLowerCase().includes(q) ?? false),
      );
    }
    return result;
  });

  /** Counts per category key (including synthetic `all`), matching legacy initSidebar. */
  const categoryCounts = computed(() => {
    const counts: Record<string, number> = { all: flags.value.length };
    for (const flag of flags.value) {
      const cat = categoryFromFlag(flag);
      counts[cat] = (counts[cat] || 0) + 1;
    }
    return counts;
  });

  function rebuildCategories(list: SystemFlag[]): void {
    const preferred = [
      'all',
      'app',
      'api',
      'database',
      'cache',
      'ai',
      'monitoring',
      'scheduler',
      'todo',
      'general',
    ];
    const present = new Set<string>();
    for (const flag of list) {
      present.add(categoryFromFlag(flag));
    }
    // Match legacy: only preferred taxonomy keys (unknown categories appear under `all` only).
    categories.value = preferred.filter((c) => c === 'all' || present.has(c));
  }

  async function fetchFlags() {
    loading.value = true;
    error.value = '';
    try {
      const res = await api.get('/api/v2/system/flags');
      if (!res.ok) {
        error.value = extractApiError(res.data, `加载系统标志失败 (${res.status})`);
        toast.showToast('error', error.value);
        flags.value = [];
        categories.value = [];
        return;
      }
      const list = unwrapFlagsPayload(res.data);
      flags.value = list;
      rebuildCategories(list);
    } catch (e) {
      error.value = e instanceof Error ? e.message : '加载系统标志失败';
      toast.showToast('error', error.value);
      flags.value = [];
      categories.value = [];
    } finally {
      loading.value = false;
    }
  }

  async function updateFlag(path: string, value: unknown) {
    const normalizedPath = typeof path === 'string' ? path.trim() : '';
    if (!normalizedPath) {
      toast.showToast('error', '配置路径无效');
      return;
    }

    const target = flags.value.find((f) => f.path === normalizedPath);
    if (target?.masked) {
      toast.showToast('error', '敏感配置不可修改');
      return;
    }

    // Preserve previous value for rollback on failure (do not clear list).
    const previousValue = target ? target.value : undefined;
    if (target) {
      target.value = value;
    }

    try {
      const res = await api.patch('/api/v2/system/flags', { path: normalizedPath, value });
      if (!res.ok) {
        if (target) {
          target.value = previousValue;
        }
        toast.showToast('error', extractApiError(res.data, `标志更新失败 (${res.status})`));
        return;
      }
      toast.showToast('success', '标志更新成功');

      // Prefer server-returned value when present; otherwise refresh full list.
      const body = res.data;
      const data = isRecord(body) && isRecord(body.data) ? body.data : isRecord(body) ? body : null;
      if (target && data && 'value' in data) {
        target.value = data.value;
        if (typeof data.masked === 'boolean') {
          target.masked = data.masked;
        }
      } else {
        await fetchFlags();
      }
    } catch (e) {
      if (target) {
        target.value = previousValue;
      }
      toast.showToast('error', `标志更新异常: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function fetchAirportContext() {
    const res = await api.get('/api/v2/system/airport-context');
    if (!res.ok) {
      throw new Error(extractApiError(res.data, `加载机场上下文失败 (${res.status})`));
    }
    return res.data;
  }

  onMounted(() => fetchFlags());

  return {
    loading,
    error,
    flags: filteredFlags,
    categoryCounts,
    categories,
    activeCategory,
    searchQuery,
    editingFlag,
    fetchFlags,
    updateFlag,
    fetchAirportContext,
  };
}
