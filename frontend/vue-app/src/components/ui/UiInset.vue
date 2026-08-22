<script setup lang="ts">
/**
 * 嵌板（信号面 §3.7）：面上临时插入的一小块「降一级」区域。
 *
 * 它承载的是刚刚发生的一件事 —— 一次诊断、一批指标、一张失败清单、一条建议。
 * 因此：降一级到 face-page（凹下去），一根 line 收边，绝不再加影、
 * 不再换圆角、不在里面套第二块嵌板 —— 那就是套盒。
 *
 * 声只在这块内容本身有事态时给：
 *   mute   常态（诊断、指标）
 *   warn   要人看一眼（失败批次、待确认）
 *   danger 出事了（提交失败）
 * 帽上只放小节名与静谓词（多半是「关闭」）。
 */
withDefaults(defineProps<{
  title?: string;
  tone?: 'mute' | 'warn' | 'danger';
  /** 帽右侧给一枚「关闭」，点了发 dismiss，收不收由调用方决定 */
  dismissible?: boolean;
}>(), {
  title: undefined,
  tone: 'mute',
  dismissible: false,
});

const emit = defineEmits<{
  (e: 'dismiss'): void;
}>();
</script>

<template>
  <section class="ui-inset" :data-tone="tone" :aria-label="title">
    <header v-if="title || dismissible || $slots.tools" class="ui-inset__head">
      <span class="ui-inset__title">{{ title }}</span>
      <span class="ui-inset__tools">
        <slot name="tools" />
        <button
          v-if="dismissible"
          type="button"
          class="ui-inset__dismiss"
          @click="emit('dismiss')"
        >
          关闭
        </button>
      </span>
    </header>
    <slot />
  </section>
</template>

<style scoped>
.ui-inset {
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: var(--face-page);
  font-size: var(--fs-label);
  color: var(--ink);
}

.ui-inset[data-tone='warn'] {
  border-color: color-mix(in srgb, var(--warn) 32%, transparent);
  background: color-mix(in srgb, var(--warn-soft) 55%, var(--face-page));
}

.ui-inset[data-tone='danger'] {
  border-color: color-mix(in srgb, var(--danger) 32%, transparent);
  background: color-mix(in srgb, var(--danger-soft) 55%, var(--face-page));
}

.ui-inset__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
  margin-bottom: 8px;
}

.ui-inset__title {
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.ui-inset[data-tone='warn'] .ui-inset__title {
  color: var(--warn);
}

.ui-inset[data-tone='danger'] .ui-inset__title {
  color: var(--danger);
}

.ui-inset__tools {
  display: inline-flex;
  align-items: center;
  gap: var(--s2);
}

/* 关：静谓词 */
.ui-inset__dismiss {
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--r-cell);
  background: none;
  color: var(--ink-subtle);
  font-size: var(--fs-label);
  font-family: inherit;
  cursor: pointer;
  transition: color var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease);
}

.ui-inset__dismiss:hover {
  color: var(--ink);
  border-color: var(--line-strong);
}

.ui-inset__dismiss:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
