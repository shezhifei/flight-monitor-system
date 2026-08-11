<script setup lang="ts">
import ConfigFieldRow from './ConfigFieldRow.vue';
import type { ConfigFieldItem } from './configFieldTypes';

defineProps<{
  items: ConfigFieldItem[];
}>();

const emit = defineEmits<{
  (e: 'change', id: string, value: unknown): void;
}>();
</script>

<template>
  <div class="cf-list" role="list">
    <ConfigFieldRow
      v-for="item in items"
      :key="item.id"
      :item="item"
      @change="(id, value) => emit('change', id, value)"
    />
  </div>
</template>
