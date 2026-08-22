<script setup lang="ts">
/**
 * 菜单项（信号面 §2.4 谓词）：一行一个动作。
 * - 常态无底色，交感时洗一层工作面色，不位移、不发光。
 * - danger 只给真会毁东西的那一项（撤销、删除），不是「重要」的意思。
 * - mute 是「看得见、点得动，但会被拒」：权限不足时留着它，
 *   让点击去把原因说出来（吐司），比灰掉不给点更好懂。
 *   真的不该点（越限、缺前置）才用 disabled。
 * - 图标是可选的行首标记，占位固定，缺了不会让文字跳。
 *
 * 在 listbox 里（选择器、@ 补全）这一行是一个值而不是一个动作，角色换成 option，
 * 键盘游标那一项是持守：`selected` 给了就用 aria-selected 报，CSS 绑这个属性，
 * 不绑一次性 class（§2.5 判定第 1 条）。它的形照抄表当前行：行动衬 + 首缘内条。
 *
 * selected 必须显式默认成 undefined：布尔 prop 不给默认值时 Vue 会把「没传」
 * 铸成 false，于是一列谓词也会各自背上一个 aria-selected="false"——
 * 那正是 §2.5 判定第 2 条禁的「一次动作不要给它 aria」。
 */
withDefaults(defineProps<{
  tone?: 'ink' | 'danger' | 'mute';
  disabled?: boolean;
  /** menuitem 是一个动作；option 是一列值里的一个值 */
  role?: 'menuitem' | 'option';
  /** 给了就是持守：这一项是键盘游标停着的地方 */
  selected?: boolean;
}>(), {
  tone: 'ink',
  disabled: false,
  role: 'menuitem',
  selected: undefined,
});
</script>

<template>
  <button
    type="button"
    class="ui-menu__item"
    :role="role"
    :data-tone="tone"
    :disabled="disabled"
    :aria-selected="selected !== undefined ? (selected ? 'true' : 'false') : undefined"
  >
    <span v-if="$slots.icon" class="ui-menu__icon"><slot name="icon" /></span>
    <span class="ui-menu__label"><slot /></span>
  </button>
</template>

<style scoped>
.ui-menu__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: var(--h-sm);
  padding: 0 10px;
  border: none;
  border-radius: var(--r-cell);
  background: none;
  color: var(--ink);
  font-size: var(--fs-body);
  font-family: inherit;
  text-align: left;
  cursor: pointer;
}

.ui-menu__item:hover:not(:disabled) {
  background: var(--face-work);
}

/* 键盘游标停着的那一项：持守，形照抄表当前行（§2.5）——行动衬 + 首缘一条内条 */
.ui-menu__item[aria-selected='true'] {
  background: var(--act-soft);
  box-shadow: inset 2px 0 0 var(--act);
}

.ui-menu__item:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: -2px;
}

.ui-menu__item:disabled {
  color: var(--ink-muted);
  cursor: not-allowed;
}

.ui-menu__item[data-tone='mute'] {
  color: var(--ink-muted);
}

.ui-menu__item[data-tone='danger'] {
  color: var(--danger);
}

.ui-menu__item[data-tone='danger']:hover:not(:disabled) {
  background: var(--danger-soft);
}

.ui-menu__icon {
  flex: none;
  width: 16px;
  text-align: center;
  line-height: 1;
}

.ui-menu__label {
  flex: 1 1 auto;
  min-width: 0;
}
</style>
