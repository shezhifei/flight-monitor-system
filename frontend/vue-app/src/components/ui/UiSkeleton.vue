<script setup lang="ts">
/**
 * 骨架（信号面 §3.9）：等的时候先把版画出来，而不是丢一句「加载中…」。
 *
 * 只有一块砖：线 / 块 / 胶囊 / 圆。页面拿它拼出与最终布局同构的形，
 * **洗光的配方只在这一处**——各页、各骨架组件不要再自带一套 shimmer。
 * 洗光用墨兑透明（`--sk-base` / `--sk-hi`），所以深浅两色都看得见。
 * 关了动效的人只看到静底，不闪。
 *
 * 骨架不报字：不要在砖里写「加载中」，无障碍名与 aria-busy 挂在外层那一块上。
 */
withDefaults(defineProps<{
  shape?: 'line' | 'block' | 'pill' | 'circle';
  /** 不给就用形的默认宽（线满宽，胶囊 56，圆 28） */
  width?: string;
  height?: string;
}>(), {
  shape: 'line',
  width: undefined,
  height: undefined,
});
</script>

<template>
  <span
    class="ui-sk"
    :data-shape="shape"
    :style="{ width, height }"
    aria-hidden="true"
  />
</template>

<style scoped>
.ui-sk {
  display: block;
  flex: none;
  background-image: linear-gradient(
    90deg,
    var(--sk-base) 25%,
    var(--sk-hi) 50%,
    var(--sk-base) 75%
  );
  background-size: 200% 100%;
  border-radius: var(--r-cell);
  animation: ui-sk-wash 1.6s var(--ease) infinite;
}

.ui-sk[data-shape='line'] {
  width: 100%;
  height: 12px;
}

.ui-sk[data-shape='block'] {
  width: 100%;
  height: 72px;
  border-radius: var(--r-control);
}

.ui-sk[data-shape='pill'] {
  width: 56px;
  height: 22px;
  border-radius: var(--r-pill);
}

.ui-sk[data-shape='circle'] {
  width: 28px;
  aspect-ratio: 1;
  border-radius: var(--r-pill);
}

@keyframes ui-sk-wash {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* 不要动的人：只留静底 */
@media (prefers-reduced-motion: reduce) {
  .ui-sk {
    animation: none;
    background-image: none;
    background-color: var(--sk-base);
  }
}
</style>
