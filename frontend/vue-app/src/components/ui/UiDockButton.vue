<script setup lang="ts">
/**
 * 角浮抬起面板里的一条：图标 + 名 + 右缘计数。
 *
 * 它坐在 UiMenu 的 role="menu" 里，所以角色也得是菜单里的角色：
 * 一次性的入口是 menuitem；开着就一直亮的（面板开合）是 menuitemcheckbox，
 * 持守用 aria-checked 报（信号面 §2.5：持守绑 aria 属性，不绑一次性 class）。
 */
/*
 * pressed 必须显式默认成 undefined：布尔 prop 不给默认值时 Vue 会把「没传」
 * 铸成 false，于是一次性的入口（AI 洞察、协同群聊）也会背上一个持守标记 —— 
 * 那正是 §2.5 第 2 条禁的「一次动作不要给它 aria」。
 */
const props = withDefaults(defineProps<{
  label: string;
  count?: number | null;
  tone?: 'mute' | 'act' | 'ok' | 'warn' | 'danger';
  /** 给了就是持守：这一条开着，手离开也还亮着 */
  pressed?: boolean;
}>(), {
  count: undefined,
  tone: 'mute',
  pressed: undefined,
});

const emit = defineEmits<{
  click: [];
}>();
</script>

<template>
  <button
    type="button"
    class="ui-dock-btn"
    :role="props.pressed !== undefined ? 'menuitemcheckbox' : 'menuitem'"
    :data-tone="tone"
    :aria-checked="pressed !== undefined ? (pressed ? 'true' : 'false') : undefined"
    :data-on="pressed === true ? 'true' : undefined"
    @click="emit('click')"
  >
    <span class="ui-dock-label">{{ label }}</span>
    <span v-if="count !== undefined && count !== null" class="ui-dock-count">{{ count }}</span>
    <slot />
  </button>
</template>

<style scoped>
/* 角浮抬起面板内的菜单条目：图标 + 名 + 右缘计数 */
.ui-dock-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 34px;
  padding: 0 10px;
  border-radius: var(--r-cell);
  border: none;
  background: transparent;
  color: var(--ink);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-family: inherit;
  cursor: pointer;
  text-align: left;
  white-space: nowrap;
}

.ui-dock-btn:hover {
  background: color-mix(in srgb, var(--ink) 7%, transparent);
}

.ui-dock-btn:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.ui-dock-btn[data-on='true'] {
  background: var(--act-soft);
  color: var(--act);
}

/* 图标由父级经 slot 传入，比文字淡一档；持守时随四声 */
.ui-dock-btn :slotted(svg) {
  flex: none;
  color: var(--ink-subtle);
}

.ui-dock-btn[data-on='true'] :slotted(svg) {
  color: var(--act);
}

.ui-dock-count {
  margin-left: auto;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--ink-subtle);
  font-variant-numeric: tabular-nums;
}

.ui-dock-btn[data-tone='warn'] .ui-dock-count { color: var(--warn); }
.ui-dock-btn[data-tone='danger'] .ui-dock-count { color: var(--danger); }
.ui-dock-btn[data-tone='ok'] .ui-dock-count { color: var(--ok); }
.ui-dock-btn[data-tone='act'] .ui-dock-count { color: var(--act); }
</style>
