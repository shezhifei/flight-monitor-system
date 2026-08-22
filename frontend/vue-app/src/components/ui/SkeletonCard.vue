<script setup lang="ts">
import UiSkeleton from './UiSkeleton.vue';

/** 航班卡的骨架：与卡的最终版同构（声胶囊 / 航线 / 两个时刻 / 脚）。
    砖与洗光都来自 UiSkeleton，这里只负责摆位。 */
const props = defineProps<{
  count?: number;
}>();
const repeatCount = Math.max(1, props.count ?? 1);
</script>

<template>
  <div
    v-for="i in repeatCount"
    :key="i"
    class="sk-card"
    role="status"
    aria-label="加载中"
    aria-busy="true"
    aria-live="polite"
  >
    <div class="sk-card__head">
      <UiSkeleton shape="pill" width="64px" />
      <UiSkeleton shape="pill" width="48px" />
    </div>
    <UiSkeleton width="70%" height="18px" />
    <UiSkeleton width="50%" />
    <div class="sk-card__times">
      <div v-for="t in 2" :key="t" class="sk-card__time">
        <UiSkeleton width="32px" height="11px" />
        <UiSkeleton width="56px" height="20px" />
      </div>
    </div>
    <div class="sk-card__foot">
      <UiSkeleton width="80px" />
      <UiSkeleton width="60px" />
    </div>
  </div>
</template>

<style scoped>
.sk-card {
  display: flex;
  flex-direction: column;
  gap: var(--s2);
  padding: var(--s3);
  margin-bottom: var(--s3);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  background: var(--face-work);
  contain: layout style paint;
}

.sk-card__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s2);
}

.sk-card__times {
  display: flex;
  justify-content: space-between;
  gap: var(--s2);
  margin-top: var(--s2);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.sk-card__time {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
}

.sk-card__foot {
  display: flex;
  justify-content: space-between;
  gap: var(--s2);
  padding-top: var(--s2);
  border-top: 1px solid var(--line);
}
</style>
