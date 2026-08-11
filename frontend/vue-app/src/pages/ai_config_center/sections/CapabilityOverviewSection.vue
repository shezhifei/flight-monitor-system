<script setup lang="ts">
defineProps<{
  existingCapabilityRows: { name: string; current: string }[];
  entityPolicyRows: string[];
}>();
</script>

<template>
  <fieldset class="models-section">
    <legend>现有能力与实体边界</legend>
    <p class="models-section-hint">
      当前系统已经具备以下基础能力；本页只配置"该实体如何使用这些能力"。可复用资产仍应放在模型目录、MCP 注册表、Agent Skill 注册表和指标表中。
    </p>
    <div class="capability-table">
      <div
        v-for="row in existingCapabilityRows"
        :key="row.name"
        class="capability-row"
      >
        <div class="capability-name">
          {{ row.name }}
        </div>
        <div class="capability-desc">
          {{ row.current }}
        </div>
      </div>
    </div>
    <div class="entity-policy-grid" aria-label="应由实体配置表达的策略">
      <span
        v-for="item in entityPolicyRows"
        :key="item"
        class="policy-chip"
      >
        {{ item }}
      </span>
    </div>
  </fieldset>
</template>
