<script setup lang="ts">
defineProps<{
  visible?: boolean;
}>();
</script>

<template>
  <div
    v-if="visible"
    class="skeleton-detail"
    role="status"
    aria-label="加载航班详情"
  >
    <div class="skeleton-detail-header">
      <div class="skeleton-shimmer" style="width: 120px; height: 36px; border-radius: 8px;" />
      <div class="skeleton-shimmer" style="width: 80px; height: 24px; border-radius: 999px;" />
    </div>

    <div class="skeleton-detail-route">
      <div class="skeleton-shimmer" style="width: 60%; height: 20px; border-radius: 6px;" />
    </div>

    <div class="skeleton-kpi-strip">
      <div v-for="i in 4" :key="i" class="skeleton-kpi">
        <div class="skeleton-shimmer" style="width: 48px; height: 12px; border-radius: 4px;" />
        <div class="skeleton-shimmer" style="width: 36px; height: 24px; border-radius: 4px; margin-top: 6px;" />
      </div>
    </div>

    <div class="skeleton-detail-grid">
      <div v-for="i in 8" :key="i" class="skeleton-info-field">
        <div class="skeleton-shimmer" style="width: 56px; height: 11px; border-radius: 4px;" />
        <div class="skeleton-shimmer" style="width: 80%; height: 18px; border-radius: 4px; margin-top: 4px;" />
      </div>
    </div>

    <div class="skeleton-detail-section">
      <div class="skeleton-shimmer" style="width: 100px; height: 16px; border-radius: 4px; margin-bottom: 12px;" />
      <div v-for="i in 3" :key="i" class="skeleton-event-row">
        <div class="skeleton-shimmer" style="width: 80px; height: 14px; border-radius: 4px;" />
        <div class="skeleton-shimmer" style="width: 120px; height: 14px; border-radius: 4px;" />
        <div class="skeleton-shimmer" style="width: 60px; height: 14px; border-radius: 999px;" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.skeleton-detail {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 20px;
  height: 100%;
  contain: layout style paint;
}

.skeleton-detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border-light);
}

.skeleton-detail-route {
  margin-top: 4px;
}

.skeleton-kpi-strip {
  display: flex;
  justify-content: space-around;
  padding: 16px;
  background: rgba(0, 0, 0, 0.02);
  border-radius: 12px;
  gap: 8px;
}

.skeleton-kpi {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
}

.skeleton-detail-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px 12px;
}

.skeleton-info-field {
  display: flex;
  flex-direction: column;
}

.skeleton-detail-section {
  margin-top: 8px;
}

.skeleton-event-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  border-bottom: 1px solid var(--border-light);
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
