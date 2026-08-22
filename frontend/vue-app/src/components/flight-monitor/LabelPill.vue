<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { useApi } from '@/composables/useApi';
import { unwrapApiData } from '@/shared/apiEnvelope';
import UiPill from '../ui/UiPill.vue';

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
    const payload = unwrapApiData<LabelDef[]>(resp.data);
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

/**
 * 标签是标识，不是事态：后端给的 `color` 不再画出来。
 * 四声（行动/安/警/危）是唯一的色相出口，没有第五声（§2.4），
 * 所以一簇自定义标签只能是无声胶囊——面 + 墨。缺定义就退回原码。
 */
const resolvedLabels = computed(() => (props.labels || []).map((code) => {
  const def = labelDefsCache.value.get(code);
  return { code, name: def?.name || code, icon: def?.icon ?? null };
}));
</script>

<template>
  <UiPill v-for="label in resolvedLabels" :key="label.code">
    <span v-if="label.icon" aria-hidden="true">{{ label.icon }}</span>
    {{ label.name }}
    <button
      v-if="removable"
      type="button"
      class="label-pill__remove"
      :aria-label="`移除标签 ${label.name}`"
      @click.stop="$emit('remove', label.code)"
    >
      ×
    </button>
  </UiPill>
</template>

<style scoped>
/* 摘掉这个标签：胶囊里的小谓词。22px 的壳装不下 UiButton，
   所以只留最小形——字借胶囊自己的墨，交感洗一层淡墨（§4.2） */
.label-pill__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  margin-right: -2px;
  padding: 0;
  border: none;
  border-radius: var(--r-pill);
  background: none;
  color: inherit;
  font: inherit;
  line-height: 1;
  cursor: pointer;
}

.label-pill__remove:hover {
  background: color-mix(in srgb, var(--ink) 10%, transparent);
}

.label-pill__remove:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
