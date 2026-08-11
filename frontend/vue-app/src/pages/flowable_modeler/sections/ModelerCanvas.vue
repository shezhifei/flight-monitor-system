<script setup lang="ts">
defineProps<{
  hasSelectedDiagram: boolean;
}>();

import { ref } from 'vue';
const canvasRef = ref<HTMLElement | null>(null);

defineExpose({ canvasRef });
</script>

<template>
  <div class="editor-body">
    <div id="bpmn-canvas" ref="canvasRef" class="canvas-host" />
    <slot name="properties-panel" />
  </div>
</template>
