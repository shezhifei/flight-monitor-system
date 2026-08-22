<script setup lang="ts">
/**
 * 表（信号面 §3.1 第 3 层）：工作面延续下去，表头不换脸。
 *
 * 已验证的变位（原为航班监控页私有 CSS，现收编为唯一一套表角色）：
 * - 表头 = 同一张工作面 + 一根 line-strong 底线；不换底色、不做灰帽子。
 * - 行距靠行线，不靠斑马纹。
 * - 选中（持守）：`tr[aria-selected="true"]` → 行动衬 + 仅首格 2px 内条。
 * - 事态（不是选中）：`tr[data-tone="warn|danger"]` → 该声 55% 洗底，画在对象上。
 * - 交感：悬停淡墨 7%，不位移、不借行动蓝。
 *
 * 槽内可用的列工具（写在 th/td 上）：
 *   data-align="end|center"  data-mono（标识/时刻用等宽 + tabular-nums）
 *   data-w="…"（列宽）        class="ui-table__empty"（空态占位格）
 */
withDefaults(defineProps<{
  /** 无障碍名称 */
  label?: string;
  /** 表头是否钉住（主体自带滚动口时用） */
  stickyHead?: boolean;
  /** 密度：compact 给长表，default 给中短表 */
  density?: 'compact' | 'default';
}>(), {
  label: undefined,
  stickyHead: true,
  density: 'compact',
});
</script>

<template>
  <table
    class="ui-table"
    :aria-label="label"
    :data-sticky="stickyHead ? 'true' : 'false'"
    :data-density="density"
  >
    <slot />
  </table>
</template>

<style scoped>
.ui-table {
  width: 100%;
  border-collapse: collapse;
  background: transparent;
  font-size: var(--fs-label);
}

/* ---- 表头：工作面延续，一根强线收口 ---- */
.ui-table :deep(th) {
  padding: 9px 12px;
  text-align: left;
  background: var(--face-work);
  color: var(--ink-subtle);
  font-size: 11px;
  font-weight: var(--fw-medium);
  letter-spacing: 0.06em;
  white-space: nowrap;
  border-bottom: 1px solid var(--line-strong);
}

.ui-table[data-sticky='true'] :deep(thead th) {
  position: sticky;
  top: 0;
  z-index: 2;
}

.ui-table :deep(th[aria-sort]),
.ui-table :deep(th.is-sortable) {
  cursor: pointer;
  user-select: none;
}

.ui-table :deep(th[aria-sort]:hover),
.ui-table :deep(th.is-sortable:hover) {
  color: var(--ink);
}

.ui-table :deep(th[aria-sort='ascending']),
.ui-table :deep(th[aria-sort='descending']) {
  color: var(--act);
}

/* ---- 格 ---- */
.ui-table :deep(td) {
  padding: 8px 12px;
  text-align: left;
  color: var(--ink);
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}

.ui-table[data-density='default'] :deep(td) {
  padding: 11px 14px;
  font-size: var(--fs-body);
}

.ui-table[data-density='default'] :deep(th) {
  padding: 10px 14px;
}

.ui-table :deep([data-align='end']) {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.ui-table :deep([data-align='center']) {
  text-align: center;
}

/* 标识（航班号、时刻、机位、ID）用等宽；叙事句子里的数字不要换字体 */
.ui-table :deep([data-mono]) {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

/* ---- 事态：画在对象上，仅警/危洗底 ---- */
.ui-table :deep(tbody tr[data-tone='warn'] > td) {
  background: color-mix(in srgb, var(--warn-soft) 55%, transparent);
}

.ui-table :deep(tbody tr[data-tone='danger'] > td) {
  background: color-mix(in srgb, var(--danger-soft) 55%, transparent);
}

/* ---- 交感：悬停淡墨；只有可点的行才有，不可点的行不该发亮 ---- */
.ui-table :deep(tbody tr[data-hoverable]:hover > td),
.ui-table :deep(tbody tr[aria-selected]:hover > td) {
  background: color-mix(in srgb, var(--ink) 7%, transparent);
}

/* ---- 选中（持守）：行动衬 + 仅首格内条。写在交感之后：
       悬停一行已选中的行，蓝不能被洗掉，只加深一档。 ---- */
.ui-table :deep(tbody tr[aria-selected='true'] > td) {
  background: var(--act-soft);
}

.ui-table :deep(tbody tr[aria-selected='true']:hover > td) {
  background: color-mix(in srgb, var(--act) 20%, transparent);
}

.ui-table :deep(tbody tr[aria-selected='true'] > td:first-child) {
  box-shadow: inset 2px 0 0 var(--act);
}

.ui-table :deep(tbody tr:focus-visible) {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

/* ---- 空态 / 载入占位：一格铺满，居中淡墨 ---- */
.ui-table :deep(.ui-table__empty) {
  padding: var(--s5) var(--s3);
  text-align: center;
  color: var(--ink-muted);
  font-size: var(--fs-body);
}
</style>
