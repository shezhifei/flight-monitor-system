import { computed, ref } from 'vue';
import { useApi } from './useApi';

export interface FieldOverlay {
  object_name: string;
  field_name: string;
  field_type: string;
  catalog_code?: string | null;
  object_name_target?: string | null;
  required: boolean;
  list_visible: boolean;
  filterable: boolean;
  widget?: string | null;
  description?: string | null;
  visible_when?: VisibleWhenCondition | null;
  max_length?: number | null;
  min?: number | null;
  max?: number | null;
  is_active: boolean;
}

export interface FieldReferenceEntry {
  id: string;
  code?: string | null;
  name?: string | null;
}

// Server contract: field_overlay_service::validate_visible_when enforces
// { field, op, value } (op optional, defaults to eq). Legacy flat-map shapes
// are not part of the contract and are treated as always visible by the form.
export interface VisibleWhenCondition {
  field: string;
  op?: string;
  value: unknown;
}

function unwrap(payload: unknown): unknown {
  if (payload && typeof payload === 'object' && 'data' in payload) return (payload as { data?: unknown }).data;
  return payload;
}

export function useFieldOverlays() {
  const api = useApi();
  const fields = ref<FieldOverlay[]>([]);
  const loading = ref(false);
  const loadedObjects = new Set<string>();
  const catalogEntries = ref<Record<string, Array<{ code: string; name: string }>>>({});

  async function load(objectName: string, force = false) {
    if (!force && loadedObjects.has(objectName)) return;
    loading.value = true;
    try {
      const response = await api.get(`/api/v2/dispatch/resources/ontology-field-overlays?object_name=${encodeURIComponent(objectName)}`);
      const rows = unwrap(response.data);
      const next = Array.isArray(rows) ? rows as FieldOverlay[] : [];
      fields.value = [...fields.value.filter((item) => item.object_name !== objectName), ...next];
      // Catalog-backed overlays are self-contained: loading an object's
      // schema also primes the option lists required by its renderer.
      const catalogs = [...new Set(next.map(item => item.catalog_code).filter((code): code is string => Boolean(code)))];
      await Promise.all(catalogs.map(code => loadCatalog(code)));
      loadedObjects.add(objectName);
    } finally {
      loading.value = false;
    }
  }

  async function loadCatalog(code: string) {
    if (catalogEntries.value[code]) return;
    const response = await api.get(`/api/v2/dispatch/resources/metadata-catalogs/${encodeURIComponent(code)}`);
    const data = unwrap(response.data) as { entries?: unknown[] } | null;
    const entries = Array.isArray(data?.entries) ? data.entries : [];
    catalogEntries.value = {
      ...catalogEntries.value,
      [code]: entries
        .filter((item): item is { code: string; name: string } => Boolean(item && typeof item === 'object' && typeof (item as { code?: unknown }).code === 'string'))
        .map((item) => ({ code: item.code, name: typeof item.name === 'string' ? item.name : item.code })),
    };
  }

  const forObject = (objectName: string) => computed(() => fields.value.filter((item) => item.object_name === objectName && item.is_active));
  return { fields, loading, load, loadCatalog, catalogEntries, forObject };
}
