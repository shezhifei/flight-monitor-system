<script setup lang="ts">
import type { AiEntitySummary } from '../aiConfigApi';
import SvgIcon from '../../../components/ui/SvgIcon.vue';

defineProps<{
  entities: AiEntitySummary[];
  selectedEntityId: string;
  entitySearch: string;
  modelsLoading: boolean;
}>();
const emit = defineEmits<{
  'update:entitySearch': [value: string];
  selectEntity: [id: string];
  refresh: [];
}>();
</script>

<template>
  <aside class="models-side">
    <div class="models-side-header">
      <span class="models-side-title">实体列表</span>
      <button
        type="button"
        class="btn btn-sm btn-secondary"
        :disabled="modelsLoading"
        @click="emit('refresh')"
      >
        <SvgIcon src="/frontend/icons/refresh.svg" :size="14" style="vertical-align: -2px;" />
        刷新
      </button>
    </div>
    <input
      :value="entitySearch"
      type="text"
      class="form-input"
      placeholder="搜索实体 ID..."
      aria-label="搜索实体"
      @input="emit('update:entitySearch', ($event.target as HTMLInputElement).value)"
    >
    <div class="models-side-list">
      <button
        v-for="entity in entities"
        :key="entity.id"
        type="button"
        class="models-side-item"
        :class="{ 'is-active': entity.id === selectedEntityId }"
        @click="emit('selectEntity', entity.id)"
      >
        {{ entity.id }}
      </button>
      <div v-if="entities.length === 0" class="empty-state-inline">
        暂无实体
      </div>
    </div>
  </aside>
</template>
