<template>
  <span
    v-for="label in resolvedLabels"
    :key="label.code"
    class="label-pill"
    :style="{ backgroundColor: label.color + '20', color: label.color, borderColor: label.color + '40' }"
    :title="label.name"
  >
    <span v-if="label.icon" class="label-icon">{{ label.icon }}</span>
    {{ label.name }}
    <button
      v-if="removable"
      class="label-remove"
      :style="{ color: label.color }"
      title="移除标签"
      @click.stop="$emit('remove', label.code)"
    >×</button>
  </span>
</template>

<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { useApi } from '@/composables/useApi';

export interface LabelDef {
  code: string;
  name: string;
  color: string;
  icon?: string | null;
  scope: string;
  category: string;
}

const props = defineProps<{
  labels: string[];
  removable?: boolean;
}>();
const api = useApi();

defineEmits<{
  remove: [code: string];
}>();

// Label definitions cache (singleton)
const labelDefsCache = ref<Map<string, LabelDef>>(new Map());
let cacheLoaded = false;

async function loadLabelDefs() {
  if (cacheLoaded) return;
  try {
    const resp = await api.get<{ success?: boolean; data?: LabelDef[] } | LabelDef[]>('/api/v2/labels');
    const payload = Array.isArray(resp.data) ? resp.data : resp.data?.data;
    if (resp.ok && Array.isArray(payload)) {
      const map = new Map<string, LabelDef>();
      for (const d of payload) {
        map.set(d.code, d);
      }
      labelDefsCache.value = map;
      cacheLoaded = true;
    }
  } catch {
    // Silently fail — labels will show raw codes
  }
}

onMounted(loadLabelDefs);

const resolvedLabels = computed(() => {
  return (props.labels || []).map((code) => {
    const def = labelDefsCache.value.get(code);
    return def || { code, name: code, color: '#6B7280', icon: null, scope: 'flight', category: 'custom' };
  });
});
</script>

<style scoped>
.label-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 10px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 500;
  border: 1px solid;
  line-height: 1.6;
  white-space: nowrap;
  transition: all 0.15s ease;
}

.label-pill:hover {
  filter: brightness(0.95);
}

.label-icon {
  font-size: 11px;
}

.label-remove {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0 0 0 2px;
  opacity: 0.6;
  transition: opacity 0.15s;
}

.label-remove:hover {
  opacity: 1;
}
</style>
