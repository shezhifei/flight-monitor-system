<script setup lang="ts">
import { computed } from 'vue';
import type { Flight } from '@/types/bindings';
import UiFacts, { type Fact } from '../../ui/UiFacts.vue';

type DetailItem = { label: string; value: string; field?: string; type?: string; readonly?: boolean };

const props = defineProps<{
  flight: Flight | null;
  detailItems: DetailItem[];
}>();

const emit = defineEmits<{
  (e: 'edit-remark', flightId: string, field: string, value: string): void;
  (e: 'edit-field', flightId: string, field: string, type: string, value: string): void;
}>();

/** 时刻是标识，用等宽（§2.4）；其余按正文走。名值对的形全部归 UiFacts（§3.2）。 */
const facts = computed<Fact[]>(() => props.detailItems.map((item) => ({
  label: item.label,
  value: item.value,
  mono: item.type === 'datetime-local',
})));

function isEditable(item: DetailItem | undefined): boolean {
  if (!item) return false;
  if (item.field === 'aircraft_check_remarks') return true;
  return item.type === 'datetime-local' && !item.readonly;
}

function onEdit(item: DetailItem): void {
  if (!item.field) return;
  const flightId = String(props.flight?.flight_id || '');
  if (item.field === 'aircraft_check_remarks') {
    emit('edit-remark', flightId, item.field, String(props.flight?.aircraft_check_remarks || ''));
    return;
  }
  const raw = (props.flight as unknown as Record<string, unknown> | null)?.[item.field];
  emit('edit-field', flightId, item.field, 'datetime-local', String(raw ?? ''));
}
</script>

<template>
  <section class="detail-card info-grid">
    <UiFacts :items="facts">
      <!-- 能改的那一格，值本身就是谓词：真的用 button，键盘也够得着 -->
      <template #value="{ index, text }">
        <button
          v-if="isEditable(detailItems[index])"
          type="button"
          class="info-grid__edit"
          :aria-label="`修改${detailItems[index].label}`"
          @click="onEdit(detailItems[index])"
        >
          {{ text }}
        </button>
        <template v-else>
          {{ text }}
        </template>
      </template>
    </UiFacts>
  </section>
</template>

<style scoped>
.info-grid {
  flex: 1;
  padding: 16px;
  overflow-y: auto;
}

/* 值是谓词时只多一条虚下线报「可以改」，字号、颜色仍归事实格；
   交感洗一层淡墨，不借行动蓝（§4.2） */
.info-grid__edit {
  margin: -2px -4px;
  padding: 2px 4px;
  border: none;
  border-radius: var(--r-cell);
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  text-decoration: underline dotted var(--line-strong);
  text-underline-offset: 3px;
  cursor: pointer;
}

.info-grid__edit:hover {
  background: color-mix(in srgb, var(--ink) 10%, transparent);
  text-decoration-color: var(--ink-subtle);
}

.info-grid__edit:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
