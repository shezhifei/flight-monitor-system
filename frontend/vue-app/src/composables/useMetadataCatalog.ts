import { computed, ref } from 'vue';
import { useApi } from './useApi';
import { useToast } from './useToast';

export interface MetadataCatalog {
  code: string;
  name: string;
  description?: string | null;
  is_open: boolean;
  is_ordered: boolean;
  system_owned: boolean;
  is_active: boolean;
}

export interface MetadataCatalogEntry {
  catalog_code: string;
  code: string;
  name: string;
  rank?: number | null;
  payload?: Record<string, unknown>;
  is_active: boolean;
  source: 'manual' | 'ingest';
}

export interface MetadataCatalogDetail extends MetadataCatalog {
  entries: MetadataCatalogEntry[];
}

export interface CatalogFormData {
  code: string;
  name: string;
  description: string;
  is_open: boolean;
  is_ordered: boolean;
}

export interface EntryFormData {
  code: string;
  name: string;
  rank: string;
}

export type MetadataCatalogModal =
  | { kind: 'none' }
  | { kind: 'catalog'; item?: MetadataCatalog }
  | { kind: 'entry'; catalog: MetadataCatalog; item?: MetadataCatalogEntry };

function unwrap<T>(payload: unknown): T | null {
  if (payload && typeof payload === 'object' && 'data' in (payload as Record<string, unknown>)) {
    return ((payload as Record<string, unknown>).data ?? null) as T | null;
  }
  return (payload ?? null) as T | null;
}

function unwrapList(payload: unknown): unknown[] {
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
        error?: string | { message?: string };
      };
      if (typeof body.error === 'object' && body.error?.message) return body.error.message;
      if (typeof body.error === 'string' && body.error) return body.error;
      return body.message || fallback;
    }
  } catch {
    /* ignore */
  }
  return fallback;
}

export function useMetadataCatalog() {
  const api = useApi();
  const toast = useToast();
  const catalogs = ref<MetadataCatalog[]>([]);
  const selectedCode = ref('');
  const detail = ref<MetadataCatalogDetail | null>(null);
  const loading = ref(false);
  const saving = ref(false);
  const search = ref('');
  const modal = ref<MetadataCatalogModal>({ kind: 'none' });
  const catalogForm = ref<CatalogFormData>({
    code: '',
    name: '',
    description: '',
    is_open: false,
    is_ordered: false,
  });
  const entryForm = ref<EntryFormData>({ code: '', name: '', rank: '' });

  const selected = computed(() => catalogs.value.find((c) => c.code === selectedCode.value) ?? null);
  const filteredEntries = computed(() => {
    const q = search.value.trim().toLowerCase();
    const entries = detail.value?.entries ?? [];
    if (!q) return entries;
    return entries.filter(
      (e) => e.code.toLowerCase().includes(q) || e.name.toLowerCase().includes(q),
    );
  });

  async function loadCatalogs() {
    loading.value = true;
    try {
      const res = await api.get('/api/v2/dispatch/metadata-catalogs?include_inactive=true');
      if (!res.ok) {
        toast.show('error', await extractErrorMessage(res.response, '加载码表失败'));
        return;
      }
      catalogs.value = unwrapList(res.data) as MetadataCatalog[];
      if (!selectedCode.value && catalogs.value[0]) {
        await selectCatalog(catalogs.value[0].code);
      } else if (selectedCode.value) {
        await selectCatalog(selectedCode.value);
      }
    } finally {
      loading.value = false;
    }
  }

  async function selectCatalog(code: string) {
    selectedCode.value = code;
    const res = await api.get(`/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(code)}?include_inactive=true`);
    if (!res.ok) {
      toast.show('error', await extractErrorMessage(res.response, '加载码表项失败'));
      detail.value = null;
      return;
    }
    detail.value = unwrap<MetadataCatalogDetail>(res.data);
  }

  function openCatalogModal(item?: MetadataCatalog) {
    catalogForm.value = {
      code: item?.code ?? '',
      name: item?.name ?? '',
      description: item?.description ?? '',
      is_open: item?.is_open ?? false,
      is_ordered: item?.is_ordered ?? false,
    };
    modal.value = { kind: 'catalog', item };
  }

  function openEntryModal(item?: MetadataCatalogEntry) {
    const catalog = selected.value;
    if (!catalog) return;
    entryForm.value = {
      code: item?.code ?? '',
      name: item?.name ?? '',
      rank: item?.rank != null ? String(item.rank) : '',
    };
    modal.value = { kind: 'entry', catalog, item };
  }

  function closeModal() {
    modal.value = { kind: 'none' };
  }

  async function saveCurrentModal() {
    if (modal.value.kind === 'catalog') {
      await saveCatalog();
    } else if (modal.value.kind === 'entry') {
      await saveEntry();
    }
  }

  async function saveCatalog() {
    saving.value = true;
    try {
      const editing = modal.value.kind === 'catalog' ? modal.value.item : undefined;
      const body = {
        code: catalogForm.value.code.trim(),
        name: catalogForm.value.name.trim(),
        description: catalogForm.value.description.trim() || null,
        is_open: catalogForm.value.is_open,
        is_ordered: catalogForm.value.is_ordered,
      };
      const res = editing
        ? await api.patch(`/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(editing.code)}`, {
            name: body.name,
            description: body.description,
            is_open: body.is_open,
            is_ordered: body.is_ordered,
          })
        : await api.post('/api/v2/dispatch/metadata-catalogs', body);
      if (!res.ok) {
        toast.show('error', await extractErrorMessage(res.response, '保存码表失败'));
        return;
      }
      toast.show('success', editing ? '码表已更新' : '码表已创建');
      closeModal();
      await loadCatalogs();
    } finally {
      saving.value = false;
    }
  }

  async function saveEntry() {
    const catalog = selected.value;
    if (!catalog) return;
    saving.value = true;
    try {
      const editing = modal.value.kind === 'entry' ? modal.value.item : undefined;
      const rankRaw = entryForm.value.rank.trim();
      const rank = rankRaw === '' ? null : Number(rankRaw);
      if (rankRaw !== '' && Number.isNaN(rank)) {
        toast.show('error', '排序必须是数字');
        return;
      }
      const res = editing
        ? await api.patch(
            `/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(catalog.code)}/entries/${encodeURIComponent(editing.code)}`,
            { name: entryForm.value.name.trim(), rank },
          )
        : await api.post(`/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(catalog.code)}/entries`, {
            code: entryForm.value.code.trim(),
            name: entryForm.value.name.trim(),
            rank,
          });
      if (!res.ok) {
        toast.show('error', await extractErrorMessage(res.response, '保存码表项失败'));
        return;
      }
      toast.show('success', editing ? '码表项已更新' : '码表项已创建');
      closeModal();
      await selectCatalog(catalog.code);
    } finally {
      saving.value = false;
    }
  }

  async function setCatalogActive(item: MetadataCatalog, next: boolean) {
    const action = next ? 'activate' : 'deactivate';
    const res = await api.post(`/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(item.code)}/${action}`);
    if (!res.ok) {
      toast.show('error', await extractErrorMessage(res.response, '更新码表状态失败'));
      return;
    }
    await loadCatalogs();
  }

  async function setEntryActive(item: MetadataCatalogEntry, next: boolean) {
    const action = next ? 'activate' : 'deactivate';
    const res = await api.post(
      `/api/v2/dispatch/metadata-catalogs/${encodeURIComponent(item.catalog_code)}/entries/${encodeURIComponent(item.code)}/${action}`,
    );
    if (!res.ok) {
      toast.show('error', await extractErrorMessage(res.response, '更新码表项状态失败'));
      return;
    }
    await selectCatalog(item.catalog_code);
  }

  return {
    catalogs,
    selectedCode,
    selected,
    detail,
    filteredEntries,
    loading,
    saving,
    search,
    modal,
    catalogForm,
    entryForm,
    loadCatalogs,
    selectCatalog,
    openCatalogModal,
    openEntryModal,
    closeModal,
    saveCurrentModal,
    setCatalogActive,
    setEntryActive,
  };
}
