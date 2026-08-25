<script setup lang="ts">
import { computed } from 'vue';
import { splitChatMentionSegments } from './splitChatMentions';

const props = defineProps<{
  content: string;
}>();

const segments = computed(() => splitChatMentionSegments(props.content ?? ''));
</script>

<template>
  <span class="chat-mention-body">
    <template v-for="(seg, i) in segments" :key="i">
      <mark v-if="seg.type === 'mention'" class="chat-mention">{{ seg.value }}</mark>
      <template v-else>{{ seg.value }}</template>
    </template>
  </span>
</template>

<style scoped>
.chat-mention-body {
  white-space: pre-wrap;
}

mark.chat-mention {
  background: var(--act-soft);
  color: var(--act);
  font-weight: var(--fw-medium);
  padding: 0 2px;
  border-radius: var(--r-cell);
}
</style>
