<script setup lang="ts">
/**
 * 角浮悬钮（信号面 §3.6 坞簇）：一页一颗主声悬钮。
 *
 * - 形：主声实底、圆形、中投影；hover 有位移，focus 有声环。
 * - 徽记：收起时只报一个数，声跟着最急的那一件事走（danger/warn/mute），
 *   工具箱展开后由每条入口自己报数，徽记退场（把 count 置 null）。
 * - 落点（fixed 定位、离边距离）归调用页的 scoped 类，本组件只给形。
 */
withDefaults(defineProps<{
  /** 无障碍名称 */
  label: string;
  /** 徽记数字；null 或 0 不显 */
  count?: number | null;
  /** 徽记的声 */
  tone?: 'danger' | 'warn' | 'mute';
  /** 是否展开了随行菜单（画持守环、报 aria-expanded） */
  expanded?: boolean;
  /** 是否带随行菜单（报 aria-haspopup） */
  haspopup?: boolean;
  disabled?: boolean;
}>(), {
  count: null,
  tone: 'danger',
  expanded: false,
  haspopup: false,
  disabled: false,
});
</script>

<template>
  <button
    type="button"
    class="ui-fab"
    :aria-label="label"
    :aria-expanded="haspopup ? (expanded ? 'true' : 'false') : undefined"
    :aria-haspopup="haspopup ? 'menu' : undefined"
    :disabled="disabled"
  >
    <slot />
    <span v-if="count && count > 0" class="ui-fab__badge" :data-tone="tone">
      {{ count > 99 ? '99+' : count }}
    </span>
  </button>
</template>

<style scoped>
.ui-fab {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border: none;
  border-radius: 50%;
  background: var(--act);
  color: var(--act-on);
  cursor: pointer;
  box-shadow: var(--shadow-md);
  transition: transform var(--t-fast) var(--ease);
}

.ui-fab:hover {
  transform: translateY(-1px);
}

.ui-fab:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ui-fab[aria-expanded='true'] {
  box-shadow: var(--shadow-md), inset 0 0 0 2px color-mix(in srgb, var(--act-on) 35%, transparent);
}

.ui-fab:disabled {
  cursor: not-allowed;
  background: color-mix(in srgb, var(--ink) 16%, transparent);
  color: var(--ink-muted);
}

/* 徽记的数字踩字阶最低那一档；方圆与偏移是角标自己的几何 */
.ui-fab__badge {
  position: absolute;
  top: -4px;
  right: -4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 var(--s1);
  border-radius: var(--r-pill);
  font-family: var(--mono);
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  font-variant-numeric: tabular-nums;
}

/* 徽记的声跟着它报的那一件事，不一律染危 */
.ui-fab__badge[data-tone='danger'] {
  background: var(--danger);
  color: var(--danger-on);
}

.ui-fab__badge[data-tone='warn'] {
  background: var(--warn);
  color: var(--warn-on);
}

/* 未读只是个数，不是事态：面 + 墨，不出声 */
.ui-fab__badge[data-tone='mute'] {
  background: var(--face-raised);
  color: var(--ink);
}
</style>
