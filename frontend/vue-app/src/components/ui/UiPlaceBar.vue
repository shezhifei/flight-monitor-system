<script setup lang="ts">
/**
 * 地点条（信号面 §3.1 第 1 层）：面包屑只报地点，不报过滤；
 * 右侧是当前谓词下的读出（计数）与页面级 meta（连接状态、刷新等）。
 */
defineProps<{
  crumbs: { label: string; href?: string }[];
  countLabel?: string;
}>();
</script>

<template>
  <div class="ui-place">
    <div class="ui-place__row">
      <nav class="ui-place__crumbs" aria-label="当前位置">
        <template v-for="(crumb, index) in crumbs" :key="crumb.label">
          <span v-if="index > 0" class="ui-place__sep" aria-hidden="true">/</span>
          <a v-if="crumb.href" class="ui-place__link" :href="crumb.href">{{ crumb.label }}</a>
          <b v-else class="ui-place__here" aria-current="page">{{ crumb.label }}</b>
        </template>
      </nav>
      <div class="ui-place__side">
        <span v-if="countLabel" class="ui-place__state">{{ countLabel }}</span>
        <div class="ui-place__meta">
          <slot name="meta" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ui-place {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--s3);
  padding: 12px 16px 0;
  font-size: var(--fs-label);
}

.ui-place__row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--s3);
  flex: 1 1 auto;
  min-width: 0;
}

.ui-place__crumbs {
  display: flex;
  align-items: baseline;
  gap: 8px;
  color: var(--ink-muted);
  min-width: 0;
}

.ui-place__sep {
  color: var(--ink-muted);
}

.ui-place__link {
  color: var(--ink-muted);
  text-decoration: none;
  border-radius: var(--r-cell);
}

.ui-place__link:hover {
  color: var(--ink);
}

.ui-place__link:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ui-place__here {
  color: var(--ink);
  font-weight: var(--fw-medium);
  font-size: var(--fs-section);
}

.ui-place__side {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
  justify-content: flex-end;
}

.ui-place__state {
  color: var(--ink-muted);
  font-size: var(--fs-label);
  font-variant-numeric: tabular-nums;
  font-family: var(--mono);
}

.ui-place__meta {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
}
</style>
