<template>
  <div class="workspace-page nl-query-page">
    <div id="header-host" />
    <div id="breadcrumb-host" />
    <AiReactEntryShell :entry-name="'nl_query'" surface="page" />
    <ThemeToggle />
  </div>
</template>

<script setup lang="ts">
import '@/styles/main.css';
import ThemeToggle from '@/components/ui/ThemeToggle.vue';
import AiReactEntryShell from '@/components/ai/AiReactEntryShell.vue';
</script>

<style scoped>
.workspace-page.nl-query-page {
  min-height: 100vh;
}
</style>
