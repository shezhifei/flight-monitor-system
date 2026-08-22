<script setup lang="ts">
import UiReadout from './UiReadout.vue';

export type ReadoutItem = {
  label: string;
  value: string | number | null | undefined;
  tone?: 'ink' | 'act' | 'ok' | 'warn' | 'danger';
  unit?: string;
  id?: string;
};

/**
 * 读数条（信号面 §3.2）：一排读出，分隔靠细线，不靠卡片。
 * 它是常驻骨架的一部分，跟地点条、工具条同一张工作面；
 * 禁止把它做成一排 KPI 卡（那就是套盒 + 第二层框）。
 */
withDefaults(defineProps<{
  items?: ReadoutItem[];
  label?: string;
  /** 密度：dense 用于表上方一行读出；roomy 用于仪表盘首屏 */
  density?: 'dense' | 'roomy';
}>(), {
  items: undefined,
  label: undefined,
  density: 'dense',
});
</script>

<template>
  <div class="ui-readouts" :data-density="density" :aria-label="label" role="group">
    <UiReadout
      v-for="item in items"
      :key="item.label"
      :id="item.id"
      :label="item.label"
      :value="item.value"
      :tone="item.tone"
      :unit="item.unit"
    />
    <slot />
  </div>
</template>

<style scoped>
.ui-readouts {
  display: flex;
  align-items: stretch;
  flex-wrap: wrap;
  gap: 0 var(--s4);
  padding: 10px 16px 12px;
}

.ui-readouts[data-density='roomy'] {
  gap: var(--s3) var(--s5);
  padding: var(--s3) 16px 16px;
}

/* 组间只用一根细线，不再画框 */
.ui-readouts > * + * {
  padding-left: var(--s4);
  border-left: 1px solid var(--line);
}

.ui-readouts[data-density='roomy'] > * + * {
  padding-left: var(--s5);
}

@media (max-width: 768px) {
  .ui-readouts {
    gap: var(--s3);
  }

  .ui-readouts > * + * {
    padding-left: 0;
    border-left: 0;
  }
}
</style>
