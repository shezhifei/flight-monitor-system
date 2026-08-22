<script setup lang="ts">
import UiMenu from '../../../components/ui/UiMenu.vue';
import UiMenuItem from '../../../components/ui/UiMenuItem.vue';

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
    <UiMenu
      v-if="isOpen"
      id="flightCellContextMenu"
      :x="x"
      :y="y"
      min-width="200px"
      label="单元格操作"
      @click.stop
      @contextmenu.prevent
    >
      <UiMenuItem
        v-if="multi"
        :disabled="overLimit"
        :title="overLimit ? '一次最多修改 200 个单元格，请缩小选区' : undefined"
        @click.stop="!overLimit && emit('batch-edit')"
      >
        批量修改「{{ fieldLabel }}」({{ selectedCount }})
      </UiMenuItem>
      <UiMenuItem @click.stop="emit('single-edit')">
        {{ multi ? '仅修改此单元格' : `修改「${fieldLabel}」` }}
      </UiMenuItem>
      <UiMenuItem
        v-if="canRevoke && !multi"
        tone="danger"
        @click.stop="emit('revoke')"
      >
        撤销此时间
      </UiMenuItem>
      <UiMenuItem
        v-if="selectedCount > 0"
        @click.stop="emit('clear-selection')"
      >
        清除选择
      </UiMenuItem>
    </UiMenu>
  </teleport>
</template>
