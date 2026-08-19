<script setup lang="ts">
/**
 * 工具条（信号面 §3.1 第 2 层）：寻左决右，一条，常驻不随表滚。
 * 寻簇只放谓词（搜索、过滤）；决簇放动词。两簇之间空隙 + 一根细线。
 */
withDefaults(defineProps<{
  seekLabel?: string;
  solveLabel?: string;
}>(), {
  seekLabel: '查找',
  solveLabel: '操作',
});
</script>

<template>
  <div class="ui-toolbar">
    <div class="ui-toolbar__cluster" role="group" :aria-label="seekLabel">
      <slot name="seek" />
    </div>
    <div class="ui-toolbar__cluster" role="group" :aria-label="solveLabel">
      <slot name="solve" />
    </div>
  </div>
</template>

<style scoped>
.ui-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s3);
  flex-wrap: wrap;
  min-height: 44px;
  padding: 8px 16px 12px;
}

.ui-toolbar__cluster {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  min-width: 0;
}

.ui-toolbar__cluster + .ui-toolbar__cluster {
  margin-left: var(--s3);
  padding-left: var(--s3);
  border-left: 1px solid var(--line);
}

@media (max-width: 768px) {
  .ui-toolbar__cluster + .ui-toolbar__cluster {
    margin-left: 0;
    padding-left: 0;
    border-left: 0;
  }
}
</style>
