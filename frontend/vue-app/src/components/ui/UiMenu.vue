<script setup lang="ts">
import { computed } from 'vue';

/**
 * 菜单（信号面 §3.6）：一列谓词。抬起面 + 一根线 + 一层影，别的什么都不加。
 *
 * 两种落法：
 *   锚定（溢出菜单）—— 外层给相对定位，菜单贴着扳机；
 *   定点（右键菜单）—— 给 x / y，自己 fixed 到光标处（配 Teleport 用）。
 *
 * 项的形一律由 UiMenuItem 给；要分组就在槽里写一个 <hr>，
 * 只落一根 line，不留大间距、不加小标题。
 *
 * 形只此一套，角色分两档：一列谓词是 menu，选择器/补全展开的那一列值是
 * listbox（项跟着换成 option，当前项用 aria-selected 报持守，见 §2.5）。
 */
const props = withDefaults(defineProps<{
  label?: string;
  /** menu 是一列谓词；listbox 是选择器展开的一列值 */
  role?: 'menu' | 'listbox';
  /** 定点落法：视口坐标，二者同时给才生效 */
  x?: number;
  y?: number;
  minWidth?: string;
}>(), {
  label: undefined,
  role: 'menu',
  x: undefined,
  y: undefined,
  minWidth: '160px',
});

const isPinned = computed(() => props.x !== undefined && props.y !== undefined);

const style = computed(() => (
  isPinned.value
    ? { minWidth: props.minWidth, top: `${props.y}px`, left: `${props.x}px` }
    : { minWidth: props.minWidth }
));
</script>

<template>
  <div
    class="ui-menu"
    :role="role"
    :aria-label="label"
    :data-pinned="isPinned ? 'true' : undefined"
    :style="style"
  >
    <slot />
  </div>
</template>

<style scoped>
.ui-menu {
  display: flex;
  flex-direction: column;
  padding: 4px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  background: var(--face-raised);
  box-shadow: var(--shadow-md);
}

.ui-menu[data-pinned='true'] {
  position: fixed;
  z-index: var(--z-menu);
}

/* 分组：一根线，仅此 */
.ui-menu :deep(hr) {
  height: 1px;
  margin: 4px 0;
  border: 0;
  background: var(--line);
}
</style>
