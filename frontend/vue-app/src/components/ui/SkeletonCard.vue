<script setup lang="ts">
const props = defineProps<{
  count?: number;
}>();
const repeatCount = Math.max(1, props.count ?? 1);
</script>

<template>
  <div
    v-for="i in repeatCount"
    :key="i"
    class="skeleton-card"
    role="status"
    aria-label="加载中"
    aria-live="polite"
  >
    <div class="skeleton-header">
      <div class="skeleton-pill skeleton-shimmer" style="width: 64px; height: 22px;" />
      <div class="skeleton-pill skeleton-shimmer" style="width: 48px; height: 22px; margin-left: auto;" />
    </div>
    <div class="skeleton-route skeleton-shimmer" style="width: 70%; height: 18px; margin-top: 12px;" />
    <div class="skeleton-meta skeleton-shimmer" style="width: 50%; height: 14px; margin-top: 8px;" />
    <div class="skeleton-times">
      <div class="skeleton-time-block">
        <div class="skeleton-shimmer" style="width: 32px; height: 11px;" />
        <div class="skeleton-shimmer" style="width: 56px; height: 20px; margin-top: 4px;" />
      </div>
      <div class="skeleton-time-block">
        <div class="skeleton-shimmer" style="width: 32px; height: 11px;" />
        <div class="skeleton-shimmer" style="width: 56px; height: 20px; margin-top: 4px;" />
      </div>
    </div>
    <div class="skeleton-footer">
      <div class="skeleton-shimmer" style="width: 80px; height: 14px;" />
      <div class="skeleton-shimmer" style="width: 60px; height: 14px;" />
    </div>
  </div>
</template>

<style scoped>
.skeleton-card {
  background: linear-gradient(145deg, var(--bg-card) 0%, var(--ws-surface-muted) 100%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  padding: 16px;
  margin-bottom: 10px;
  contain: layout style paint;
}

.skeleton-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.skeleton-pill {
  border-radius: 999px;
}

.skeleton-route {
  border-radius: 6px;
}

.skeleton-meta {
  border-radius: 4px;
}

.skeleton-times {
  display: flex;
  justify-content: space-between;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border-light);
}

.skeleton-time-block {
  display: flex;
  flex-direction: column;
}

.skeleton-footer {
  display: flex;
  justify-content: space-between;
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px solid var(--border-light);
}

.skeleton-shimmer {
  background: linear-gradient(
    90deg,
    rgba(0, 0, 0, 0.06) 25%,
    rgba(0, 0, 0, 0.10) 50%,
    rgba(0, 0, 0, 0.06) 75%
  );
  background-size: 200% 100%;
  border-radius: 4px;
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
