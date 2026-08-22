<script setup lang="ts">
export type Fact = {
  label: string;
  value?: string | number | null;
  /** 标识（航班号、时刻、编号）用等宽；叙事文字不要用 */
  mono?: boolean;
};

/**
 * 事实格（信号面 §3.2）：一格一个「名 + 值」。详情面上的属性表就是它。
 *
 * 与读数（`UiReadout`）分工：读数报**数**（大号等宽，会带声）；
 * 事实格报**值**（状态、范围、创建人、时刻），字号同正文，不带声、不加图标。
 * 两者都不描边、不投影 —— 分组只用一根线，禁止做成一排小卡片。
 *
 * 值缺了就写「—」，不要留空格，也不要写「暂无数据」占一行。
 *
 * 值本身可以是一颗谓词（点一下改这一格）。那种情况用 `value` 槽接管这一格的值，
 * 名、字号、缺值写法仍归这里 —— 名值对只此一套配方，不许在外面照抄一遍。
 */
withDefaults(defineProps<{
  items?: Fact[];
  /** 列数；给 1 就是一列名值对 */
  columns?: number;
  density?: 'dense' | 'roomy';
}>(), {
  items: undefined,
  columns: 2,
  density: 'dense',
});

function displayValue(item: Fact): string {
  if (item.value === null || item.value === undefined || item.value === '') return '—';
  return String(item.value);
}
</script>

<template>
  <dl
    class="ui-facts"
    :data-density="density"
    :style="{ '--facts-cols': String(columns) }"
  >
    <div v-for="(item, index) in items" :key="item.label" class="ui-facts__cell">
      <dt>{{ item.label }}</dt>
      <dd :data-mono="item.mono ? 'true' : undefined">
        <slot
          name="value"
          :fact="item"
          :index="index"
          :text="displayValue(item)"
        >
          {{ displayValue(item) }}
        </slot>
      </dd>
    </div>
    <slot />
  </dl>
</template>

<style scoped>
.ui-facts {
  display: grid;
  grid-template-columns: repeat(var(--facts-cols, 2), minmax(0, 1fr));
  gap: 10px var(--s3);
  margin: 0;
}

.ui-facts[data-density='roomy'] {
  gap: var(--s3) var(--s4);
}

.ui-facts__cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.ui-facts dt {
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  color: var(--ink-muted);
}

/* 值里作者打的换行要留着（表单里填的整段话也是一个值），长串不撑破格 */
.ui-facts dd {
  margin: 0;
  font-size: var(--fs-body);
  color: var(--ink);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.ui-facts dd[data-mono='true'] {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}
</style>
