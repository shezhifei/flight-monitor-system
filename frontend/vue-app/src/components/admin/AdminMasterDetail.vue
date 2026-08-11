<script setup lang="ts">
/**
 * Master-detail split shell used by admin workbenches
 * (派工规则任务类型、流程设计事项列表等).
 */
withDefaults(
  defineProps<{
    listAriaLabel?: string;
    detailAriaLabel?: string;
    /** Left column width; default matches admin overview */
    listWidth?: string;
  }>(),
  {
    listAriaLabel: '概览列表',
    detailAriaLabel: '详情',
    listWidth: '260px',
  },
);
</script>

<template>
  <div
    class="admin-master-detail"
    :style="{ '--admin-md-list-width': listWidth }"
  >
    <aside class="admin-master-detail__list panel" :aria-label="listAriaLabel">
      <slot name="list" />
    </aside>
    <section class="admin-master-detail__detail panel" :aria-label="detailAriaLabel">
      <slot name="detail" />
      <slot />
    </section>
  </div>
</template>
