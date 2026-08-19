<script setup lang="ts">
import { ref, watch } from 'vue';
import { useSSE } from '@/composables/useSSE';
import UiDrawer from '@/components/ui/UiDrawer.vue';
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
    default: return 'neutral';
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
  <button
    type="button"
    class="ai-widget-fab"
    aria-label="管理员 AI 事件"
    @click="open = true"
  >
    <SvgIcon src="/frontend/icons/ai.svg" :size="22" />
    <span v-if="unreadCount > 0" class="ai-widget-badge">
      {{ unreadCount > 99 ? '99+' : unreadCount }}
    </span>
  </button>

  <UiDrawer :open="open" title="管理员 AI 助手" :width="420" @close="open = false">
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
.ai-widget-fab {
  position: fixed;
  right: 20px;
  bottom: 72px;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  border: none;
  background: var(--act);
  color: var(--act-on);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: var(--shadow-md);
  z-index: 9000;
}

.ai-widget-fab:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.ai-widget-badge {
  position: absolute;
  top: -4px;
  right: -4px;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 9px;
  background: var(--danger);
  color: #fff;
  font-size: 11px;
  line-height: 18px;
  text-align: center;
  box-sizing: border-box;
}

.ai-widget-count {
  margin: 0 0 12px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
}

.ai-widget-empty {
  margin-top: 24px;
  text-align: center;
  font-size: var(--fs-body);
  color: var(--ink-muted);
}
</style>
