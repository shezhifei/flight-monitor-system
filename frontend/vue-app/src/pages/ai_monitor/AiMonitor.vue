<template>
  <div class="ai-page">
    <div id="header-host" />
    <div id="breadcrumb-host" />
    <AiReactEntryShell
      :entry-name="'ai_monitor'"
      surface="page"
      :entry-features="['plan-board', 'subagent-tree', 'compression-notice']"
    />
    <ThemeToggle />
  </div>
</template>

<script setup lang="ts">
import '@/styles/main.css';
import AiReactEntryShell from '@/components/ai/AiReactEntryShell.vue';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
</script>

<style scoped>
.ai-page {
  min-height: 100vh;
}
</style>
