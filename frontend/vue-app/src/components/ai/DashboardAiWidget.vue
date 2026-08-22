<script setup lang="ts">
import { ref, watch } from 'vue';
import { useSSE } from '@/composables/useSSE';
import UiDrawer from '@/components/ui/UiDrawer.vue';
import UiFab from '@/components/ui/UiFab.vue';
import UiTimeline, { type UiTimelineItem } from '@/components/ui/UiTimeline.vue';
import SvgIcon from '@/components/ui/SvgIcon.vue';

interface EventItem {
  id: string;
  title: string;
  type: string;
  time: string;
}

const VISIBLE_TYPES = ['approval_required', 'approval_result', 'tool_start', 'tool_end', 'execution_end'];

const open = ref(false);
const events = ref<EventItem[]>([]);
const unreadCount = ref(0);

watch(open, (isOpen) => {
  if (isOpen) unreadCount.value = 0;
});

function toneOf(type: string): UiTimelineItem['tone'] {
  switch (type) {
    case 'approval_required': return 'warn';
    case 'tool_start': return 'act';
    case 'execution_end':
    case 'approval_result': return 'ok';
    case 'stream_error': return 'danger';
    default: return 'mute';
  }
}

function appendEvent(item: EventItem) {
  events.value = [...events.value.slice(-99), item];
  if (!open.value) unreadCount.value += 1;
}

const sse = useSSE({
  url: '/api/v2/ai/events/stream',
  clientScope: 'dashboard_admin_ai',
});

sse.on('ai_execution', (event) => {
  const data = (event as MessageEvent<string>).data;
  let payload: Record<string, unknown> = {};
  try {
    payload = JSON.parse(data) as Record<string, unknown>;
  } catch {
    return;
  }
  const runtime = payload.payload && typeof payload.payload === 'object'
    ? (payload.payload as Record<string, unknown>)
    : payload;
  const semantic = String(runtime.event || payload.type || 'ai_execution').toLowerCase();
  if (!VISIBLE_TYPES.includes(semantic)) return;
  appendEvent({
    id: `${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    title: String(runtime.message || runtime.tool_name || semantic),
    type: semantic,
    time: new Date().toLocaleTimeString(),
  });
});

sse.on('error', () => {
  appendEvent({
    id: `err_${Date.now()}`,
    title: 'AI 实时事件连接已断开，等待自动重连',
    type: 'stream_error',
    time: new Date().toLocaleTimeString(),
  });
});

void sse.connect();

function toTimeline(eventsList: EventItem[]): UiTimelineItem[] {
  return [...eventsList].reverse().map((e) => ({
    key: e.id,
    title: e.title,
    time: e.time,
    tone: toneOf(e.type),
  }));
}
</script>

<template>
  <UiFab
    class="ai-widget-fab"
    label="管理员 AI 事件"
    :count="unreadCount"
    @click="open = true"
  >
    <SvgIcon src="/frontend/icons/ai.svg" :size="22" />
  </UiFab>

  <UiDrawer
    :open="open"
    title="管理员 AI 助手"
    :width="420"
    @close="open = false"
  >
    <p class="ai-widget-count">
      事件 {{ events.length }}
    </p>
    <UiTimeline :items="toTimeline(events)" />
    <p v-if="!events.length" class="ai-widget-empty">
      暂无事件
    </p>
  </UiDrawer>
</template>

<style scoped>
/* 形归 UiFab；这里只给它在页角的落点 */
.ai-widget-fab {
  position: fixed;
  right: var(--s5);
  bottom: 72px;
  z-index: var(--z-dock);
}

.ai-widget-count {
  margin: 0 0 var(--s3);
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.ai-widget-empty {
  margin-top: var(--s4);
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}
</style>
