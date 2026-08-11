<template>
  <div class="ai-page">
    <div id="header-host" />
    <div id="breadcrumb-host" />
    <AiReactEntryShell :entry-name="'llm_eval_lab'" surface="page" />
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
