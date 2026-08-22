<script setup lang="ts">
import UiSkeleton from './UiSkeleton.vue';

/** 表行骨架：行高与格内衬跟真表一致，第 3 列是声胶囊的位。 */
const props = defineProps<{
  count?: number;
  columns?: number;
}>();
const repeatCount = Math.max(1, props.count ?? 5);
const colCount = Math.max(1, props.columns ?? 16);
</script>

<template>
  <tr
    v-for="i in repeatCount"
    :key="i"
    class="sk-row"
    role="status"
    aria-label="加载中"
    aria-busy="true"
  >
    <td v-for="c in colCount" :key="c" class="sk-row__cell">
      <UiSkeleton
        v-if="c === 3"
        shape="pill"
        width="48px"
        height="20px"
      />
      <UiSkeleton v-else :width="c === 1 ? '60px' : '80%'" height="14px" />
    </td>
  </tr>
</template>

<style scoped>
.sk-row {
  height: var(--h-lg);
}

.sk-row__cell {
  padding: 8px var(--s3);
  vertical-align: middle;
}
</style>
