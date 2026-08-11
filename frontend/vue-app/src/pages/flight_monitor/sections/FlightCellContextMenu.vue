<script setup lang="ts">
defineProps<{
  isOpen: boolean;
  x: number;
  y: number;
  multi: boolean;
  selectedCount: number;
  fieldLabel: string;
  /** Single datetime cell only — never shown for multi-select (no batch clear). */
  canRevoke?: boolean;
  overLimit?: boolean;
}>();

const emit = defineEmits<{
  (e: 'batch-edit'): void;
  (e: 'single-edit'): void;
  (e: 'revoke'): void;
  (e: 'clear-selection'): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <teleport to="body">
    <div
      v-if="isOpen"
      id="flightCellContextMenu"
      class="context-menu flight-cell-context-menu"
      role="menu"
      :style="{ top: `${y}px`, left: `${x}px` }"
      @click.stop
      @contextmenu.prevent
    >
      <button
        v-if="multi"
        type="button"
        class="context-menu-item"
        role="menuitem"
        :disabled="overLimit"
        :title="overLimit ? `一次最多修改 200 个单元格，请缩小选区` : undefined"
        @click.stop="!overLimit && emit('batch-edit')"
      >
        批量修改「{{ fieldLabel }}」({{ selectedCount }})
      </button>
      <button
        type="button"
        class="context-menu-item"
        role="menuitem"
        @click.stop="emit('single-edit')"
      >
        {{ multi ? '仅修改此单元格' : `修改「${fieldLabel}」` }}
      </button>
      <button
        v-if="canRevoke && !multi"
        type="button"
        class="context-menu-item danger-action"
        role="menuitem"
        @click.stop="emit('revoke')"
      >
        撤销此时间
      </button>
      <button
        v-if="selectedCount > 0"
        type="button"
        class="context-menu-item"
        role="menuitem"
        @click.stop="emit('clear-selection')"
      >
        清除选择
      </button>
    </div>
  </teleport>
</template>

<style scoped>
.flight-cell-context-menu {
  position: fixed;
  background-color: var(--bg-card, #fff);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.35);
  z-index: 10001;
  min-width: 200px;
  display: flex;
  flex-direction: column;
}

.flight-cell-context-menu .context-menu-item {
  background: none;
  border: none;
  padding: 10px 16px;
  text-align: left;
  color: var(--text-primary, #1D1D1F);
  cursor: pointer;
  font-size: 13px;
  transition: background-color 0.15s;
}

.flight-cell-context-menu .context-menu-item:hover:not(:disabled) {
  background-color: var(--bg-input, #f0f0f0);
}

.flight-cell-context-menu .context-menu-item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.flight-cell-context-menu .context-menu-item.danger-action {
  color: var(--color-danger, #c0392b);
}
</style>
