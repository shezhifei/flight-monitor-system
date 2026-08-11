<script setup lang="ts">
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
    class="skeleton-table-row"
    role="status"
    aria-label="加载中"
  >
    <td v-for="c in colCount" :key="c" class="skeleton-cell">
      <div
        class="skeleton-shimmer"
        :style="{
          width: c === 1 ? '60px' : c === 3 ? '48px' : '80%',
          height: c === 3 ? '20px' : '14px',
          borderRadius: c === 3 ? '999px' : '4px'
        }"
      />
    </td>
  </tr>
</template>

<style scoped>
.skeleton-table-row {
  height: 40px;
}

.skeleton-cell {
  padding: 8px 12px;
  vertical-align: middle;
}

.skeleton-shimmer {
  background: linear-gradient(
    90deg,
    rgba(0, 0, 0, 0.06) 25%,
    rgba(0, 0, 0, 0.10) 50%,
    rgba(0, 0, 0, 0.06) 75%
  );
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.6s ease-in-out infinite;
}

@media (prefers-reduced-motion: reduce) {
  .skeleton-shimmer {
    animation: none;
    background: rgba(0, 0, 0, 0.08);
  }
}

@keyframes skeleton-shimmer {
  0% {
    background-position: 200% 0;
  }
  100% {
    background-position: -200% 0;
  }
}
</style>
