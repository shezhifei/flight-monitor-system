<script setup lang="ts">
import UiSkeleton from './UiSkeleton.vue';

/** 航班详情的骨架：帽 / 航线 / 读数条 / 字段格 / 事件表，与详情最终版同构。
    读数条不画成带底的方块（§3.2 禁 KPI 卡），只用一根线与上面分开。 */
defineProps<{
  visible?: boolean;
}>();
</script>

<template>
  <div
    v-if="visible"
    class="sk-detail"
    role="status"
    aria-label="加载航班详情"
    aria-busy="true"
  >
    <div class="sk-detail__head">
      <UiSkeleton shape="block" width="120px" height="36px" />
      <UiSkeleton shape="pill" width="80px" height="24px" />
    </div>

    <UiSkeleton width="60%" height="20px" />

    <div class="sk-detail__readouts">
      <div v-for="i in 4" :key="i" class="sk-detail__readout">
        <UiSkeleton width="48px" height="11px" />
        <UiSkeleton width="36px" height="24px" />
      </div>
    </div>

    <div class="sk-detail__grid">
      <div v-for="i in 8" :key="i" class="sk-detail__field">
        <UiSkeleton width="56px" height="11px" />
        <UiSkeleton width="80%" height="18px" />
      </div>
    </div>

    <div class="sk-detail__section">
      <UiSkeleton width="100px" height="16px" />
      <div v-for="i in 3" :key="i" class="sk-detail__event">
        <UiSkeleton width="80px" height="14px" />
        <UiSkeleton width="120px" height="14px" />
        <UiSkeleton shape="pill" width="60px" height="14px" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.sk-detail {
  display: flex;
  flex-direction: column;
  gap: var(--s4);
  height: 100%;
  padding: var(--s4);
  contain: layout style paint;
}

.sk-detail__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding-bottom: var(--s3);
  border-bottom: 1px solid var(--line);
}

.sk-detail__readouts {
  display: flex;
  justify-content: space-between;
  gap: var(--s4);
  padding-top: var(--s3);
  border-top: 1px solid var(--line);
}

.sk-detail__readout {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  gap: var(--s2);
}

.sk-detail__grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--s3) var(--s3);
}

.sk-detail__field {
  display: flex;
  flex-direction: column;
  gap: var(--s1);
}

.sk-detail__section {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
}

.sk-detail__event {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  padding-bottom: var(--s3);
  border-bottom: 1px solid var(--line);
}
</style>
