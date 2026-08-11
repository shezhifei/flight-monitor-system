<script setup lang="ts">
/**
 * Search + primary create row for overview lists
 * (流程设计侧栏 / 派工规则 toolbar 可复用同一搜索样式).
 */
import SvgIcon from '@/components/ui/SvgIcon.vue';

withDefaults(
  defineProps<{
    modelValue?: string;
    placeholder?: string;
    searchAriaLabel?: string;
    createLabel?: string;
    createTitle?: string;
    createDisabled?: boolean;
    /** Icon-only + button when true (sidebar compact) */
    compactCreate?: boolean;
    showCreate?: boolean;
  }>(),
  {
    modelValue: '',
    placeholder: '搜索…',
    searchAriaLabel: '搜索',
    createLabel: '新建',
    createTitle: '新建',
    createDisabled: false,
    compactCreate: false,
    showCreate: true,
  },
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'create'): void;
  (e: 'search'): void;
}>();

function onInput(event: Event): void {
  emit('update:modelValue', (event.target as HTMLInputElement).value);
  emit('search');
}
</script>

<template>
  <div class="admin-overview-tools" :class="{ 'is-compact': compactCreate }">
    <div class="search-group admin-overview-tools__search">
      <span class="search-icon" aria-hidden="true">
        <SvgIcon src="/frontend/icons/search.svg" :size="16" />
      </span>
      <input
        class="search-input"
        type="search"
        :value="modelValue"
        :placeholder="placeholder"
        :aria-label="searchAriaLabel"
        @input="onInput"
      >
    </div>
    <button
      v-if="showCreate"
      type="button"
      class="btn btn-primary"
      :class="compactCreate ? 'admin-overview-tools__create-icon' : 'btn-sm'"
      :disabled="createDisabled"
      :title="createTitle"
      :aria-label="createTitle"
      @click="emit('create')"
    >
      <template v-if="compactCreate">+</template>
      <template v-else>{{ createLabel }}</template>
    </button>
  </div>
</template>
