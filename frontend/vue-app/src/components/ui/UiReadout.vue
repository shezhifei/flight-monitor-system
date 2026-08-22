<script setup lang="ts">
import { computed } from 'vue';

/**
 * 读数（信号面 §3.2）：一个数 + 它的名。仪表盘上的数是「读出」，不是卡片。
 * - 数用等宽 + tabular-nums，名用 12px 标。
 * - 声只在数真的说了什么的时候出场：0 条紧急不该是红的。
 * - 自己不描边、不投影、不换圆角；层次由读数条的分隔线给。
 */
const props = withDefaults(defineProps<{
  label: string;
  value: string | number | null | undefined;
  tone?: 'ink' | 'act' | 'ok' | 'warn' | 'danger';
  /** 数后面的单位/后缀，比 value 淡一档 */
  unit?: string;
  /** 值为 0 / 空时收回声，避免「0 条紧急」也发红 */
  quietWhenZero?: boolean;
  id?: string;
}>(), {
  tone: 'ink',
  unit: undefined,
  quietWhenZero: true,
  id: undefined,
});

const text = computed(() => {
  if (props.value === null || props.value === undefined || props.value === '') return '—';
  return String(props.value);
});

const isZero = computed(() => text.value === '0' || text.value === '—');

const activeTone = computed(() => (
  props.quietWhenZero && isZero.value ? 'ink' : props.tone
));
</script>

<template>
  <div class="ui-readout" :data-tone="activeTone">
    <span class="ui-readout__label">{{ label }}</span>
    <span class="ui-readout__value">
      <span :id="id" class="ui-readout__num">{{ text }}</span>
      <span v-if="unit" class="ui-readout__unit">{{ unit }}</span>
    </span>
  </div>
</template>

<style scoped>
.ui-readout {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.ui-readout__label {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
  white-space: nowrap;
}

.ui-readout__value {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}

.ui-readout__num {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
  font-size: var(--fs-page);
  font-weight: var(--fw-semibold);
  line-height: 1.15;
  color: var(--ink);
}

.ui-readout__unit {
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.ui-readout[data-tone='act'] .ui-readout__num { color: var(--act); }
.ui-readout[data-tone='ok'] .ui-readout__num { color: var(--ok); }
.ui-readout[data-tone='warn'] .ui-readout__num { color: var(--warn); }
.ui-readout[data-tone='danger'] .ui-readout__num { color: var(--danger); }
</style>
