<script setup lang="ts">
/**
 * 浮舱（信号面 §3.5）：钉在坞（UiDock）之上的常驻旁路面板。
 *
 * 它不是弹窗：没有幕（scrim）、不夺焦、不阻断底下的工作面 —— 值班时开着，
 * 一边听、一边看表。所以它只有一层抬起面 + 一根线 + 一层影：
 *   面 = face-raised（抬起）    帽 = face-work（与页面工作面同脸）
 *   身 = 唯一的滚动口          脚 = 只放主谓词
 * 里面不要再描第二道边、不要再换圆角 —— 那就是套盒。
 *
 * 落点与坞共用 --dock-right / --float-bottom / --z-float，
 * 各页不要再自己写 position: fixed 的角浮坐标。
 */
withDefaults(defineProps<{
  open: boolean;
  /** 帽上的名，同时作为 dialog 的无障碍名 */
  title: string;
  /** 名下一行小字：常用来报状态（「监听中」「已连接」） */
  subtitle?: string;
  width?: string;
  height?: string;
  /** 身是否自带滚动口；长内容用 true */
  scroll?: boolean;
}>(), {
  subtitle: undefined,
  width: 'min(420px, calc(100vw - 32px))',
  height: 'min(600px, calc(100vh - 128px))',
  scroll: true,
});

const emit = defineEmits<{
  (e: 'close'): void;
}>();
</script>

<template>
  <section
    v-if="open"
    class="ui-float"
    role="dialog"
    :aria-label="title"
    :style="{ '--float-w': width, '--float-h': height }"
  >
    <header class="ui-float__head">
      <div class="ui-float__title">
        <h2>{{ title }}</h2>
        <p v-if="subtitle">{{ subtitle }}</p>
      </div>
      <div class="ui-float__head-tools">
        <slot name="meta" />
        <button
          type="button"
          class="ui-float__close"
          aria-label="关闭"
          @click="emit('close')"
        >
          ×
        </button>
      </div>
    </header>

    <div class="ui-float__body" :data-scroll="scroll ? 'true' : 'false'">
      <slot />
    </div>

    <footer v-if="$slots.footer" class="ui-float__foot">
      <slot name="footer" />
    </footer>
  </section>
</template>

<style scoped>
.ui-float {
  position: fixed;
  right: var(--dock-right);
  bottom: var(--float-bottom);
  z-index: var(--z-float);
  display: flex;
  flex-direction: column;
  width: var(--float-w);
  max-height: var(--float-h);
  background: var(--face-raised);
  color: var(--ink);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  font-size: var(--fs-body);
  overflow: hidden;
}

/* 帽：与页面工作面同脸，一根线收口 */
.ui-float__head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding: 12px 14px;
  background: var(--face-work);
  border-bottom: 1px solid var(--line);
}

.ui-float__title h2 {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  line-height: 1.3;
}

.ui-float__title p {
  margin: 2px 0 0;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.ui-float__head-tools {
  display: flex;
  align-items: center;
  gap: var(--s2);
}

/* 关：静谓词，只有交感时才显形 */
.ui-float__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid transparent;
  border-radius: var(--r-control);
  background: none;
  color: var(--ink-subtle);
  font-size: 17px;
  line-height: 1;
  font-family: inherit;
  cursor: pointer;
  transition: color var(--t-fast) var(--ease), border-color var(--t-fast) var(--ease);
}

.ui-float__close:hover {
  color: var(--ink);
  border-color: var(--line-strong);
}

.ui-float__close:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

/* 身：唯一的滚动口 */
.ui-float__body {
  flex: 1 1 auto;
  min-height: 0;
}

.ui-float__body[data-scroll='true'] {
  overflow-y: auto;
}

/* 身不自带滚动口时，就把滚动权交给里面那一列（会话流即如此）：
   身只负责撑满剩余高度，子级用 flex: 1 + min-height: 0 自己滚。 */
.ui-float__body[data-scroll='false'] {
  display: flex;
  flex-direction: column;
}

/* 脚：只放主谓词，一根线与身分开 */
.ui-float__foot {
  flex: none;
  padding: 10px 14px;
  border-top: 1px solid var(--line);
  background: var(--face-work);
}
</style>
