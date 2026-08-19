<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import AIAssistantFloatPanel from '../../../components/flight-monitor/AIAssistantFloatPanel.vue';
import AutoCopilotVoicePanel from '../../../components/flight-monitor/AutoCopilotVoicePanel.vue';
import DispatchCollaborationChat from '../../../components/flight-monitor/DispatchCollaborationChat.vue';
import DispatchNotifyModal from '../../../components/flight-monitor/DispatchNotifyModal.vue';
import CriticalNotifyModal from '../../../components/flight-monitor/CriticalNotifyModal.vue';
import DispatchReminderModal from '../../../components/flight-monitor/DispatchReminderModal.vue';
import FlightInsightModal from '../../../components/flight-monitor/FlightInsightModal.vue';
import UiDock from '../../../components/ui/UiDock.vue';
import UiDockButton from '../../../components/ui/UiDockButton.vue';
import type { UserNotification } from '@/types/bindings';
import type { BusinessCaseTypeDefinition } from '../../../types/backend';

const props = defineProps<{
  anomalyCount: number;
  anomalySeverity: 'high' | 'medium' | 'low';
  updateMessages: string[];
  updatePanelOpen: boolean;
  notificationCount: number;
  dispatchNotifyOpen: boolean;
  dispatchChatOpen: boolean;
  flightInsightOpen: boolean;
  selectedFlightId: string | null;
  selectedFlightNo: string | undefined;
  flightNoResolver?: (flightId: string) => string | null | undefined;
  businessCaseTypes: BusinessCaseTypeDefinition[];
  criticalNotificationQueue: UserNotification[];
  sentReceiptReminderQueue: string[];
}>();

const emit = defineEmits<{
  (e: 'toggle-anomaly-pool'): void;
  (e: 'toggle-update-panel'): void;
  (e: 'close-update-panel'): void;
  (e: 'open-dispatch'): void;
  (e: 'open-chat'): void;
  (e: 'open-insight'): void;
  (e: 'close-dispatch'): void;
  (e: 'close-chat'): void;
  (e: 'close-insight'): void;
  (e: 'shift-critical-notification'): void;
  (e: 'shift-reminder'): void;
  (e: 'view-reminder-history'): void;
  (e: 'auto-copilot-created', payload: { caseIds: string[]; notificationGroupCount: number }): void;
  (e: 'toast', message: string): void;
  (e: 'error', message: string): void;
}>();

const aiAssistantOpen = ref(false);
const copilotOpen = ref(false);
const aiUnread = ref(0);

// 角浮：一页一颗主声悬钮，点开抬起临时工具箱
const dockOpen = ref(false);
const dockRoot = ref<HTMLElement | null>(null);

const fabBadge = computed(() => {
  if (props.anomalyCount > 0) return props.anomalyCount;
  if (props.notificationCount > 0) return props.notificationCount;
  return aiUnread.value;
});

function onDocPointerDown(event: PointerEvent): void {
  if (dockRoot.value && !dockRoot.value.contains(event.target as Node)) dockOpen.value = false;
}

onMounted(() => document.addEventListener('pointerdown', onDocPointerDown));
onBeforeUnmount(() => document.removeEventListener('pointerdown', onDocPointerDown));
</script>

<template>
  <div ref="dockRoot">
    <UiDock label="临时工具箱">
      <div v-if="dockOpen" class="fm-dock-panel" role="menu" aria-label="临时工具箱">
        <UiDockButton
          v-if="anomalyCount > 0"
          label="异常告警"
          :count="anomalyCount"
          :tone="anomalySeverity === 'high' ? 'danger' : 'warn'"
          @click="emit('toggle-anomaly-pool')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3 2.5 19.5h19L12 3z" /><path d="M12 10v4" /><path d="M12 17v.5" /></svg>
        </UiDockButton>
        <UiDockButton
          label="待处理更新"
          :count="updateMessages.length"
          :pressed="updatePanelOpen"
          @click="emit('toggle-update-panel')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 12a8 8 0 1 1-2.34-5.66" /><path d="M20 4v4h-4" /></svg>
        </UiDockButton>
        <UiDockButton
          label="调度网关"
          :count="notificationCount"
          :tone="notificationCount > 0 ? 'danger' : 'neutral'"
          @click="emit('open-dispatch')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9a6 6 0 1 1 12 0c0 5 2 6 2 6H4s2-1 2-6" /><path d="M10 19a2 2 0 0 0 4 0" /></svg>
        </UiDockButton>
        <UiDockButton label="AI 洞察" :count="null" @click="emit('open-insight')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z" /></svg>
        </UiDockButton>
        <UiDockButton label="协同群聊" :count="null" @click="emit('open-chat')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 5h16v11H9l-5 4z" /></svg>
        </UiDockButton>
        <UiDockButton
          label="极智 AI 指挥官"
          :count="aiUnread > 0 ? aiUnread : null"
          :pressed="aiAssistantOpen"
          @click="aiAssistantOpen = !aiAssistantOpen"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6z" /></svg>
        </UiDockButton>
        <UiDockButton label="语音事项" :pressed="copilotOpen" @click="copilotOpen = !copilotOpen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z" /><path d="M6 11a6 6 0 0 0 12 0" /><path d="M12 17v4" /></svg>
        </UiDockButton>
      </div>
      <button
        type="button"
        class="fm-dock-fab"
        aria-haspopup="menu"
        :aria-expanded="dockOpen ? 'true' : 'false'"
        aria-label="临时工具箱"
        @click="dockOpen = !dockOpen"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 4h7v7H4z" /><path d="M13 4h7v7h-7z" /><path d="M4 13h7v7H4z" /><path d="M13 13h7v7h-7z" /></svg>
        <span v-if="fabBadge > 0" class="fm-dock-fab-badge">{{ fabBadge }}</span>
      </button>
    </UiDock>
  </div>

  <section
    v-if="updatePanelOpen"
    id="updatePanel"
    class="update-panel"
    role="dialog"
    aria-modal="false"
    aria-labelledby="updatePanelTitle"
  >
    <div class="update-panel-header">
      <div id="updatePanelTitle" class="update-panel-title">
        更新面板
      </div>
      <button
        type="button"
        class="update-panel-close"
        aria-label="关闭更新面板"
        @click="emit('close-update-panel')"
      >
        ×
      </button>
    </div>
    <div class="update-panel-content">
      <template v-if="updateMessages.length">
        <p v-for="message in updateMessages" :key="message" class="update-panel-message">
          {{ message }}
        </p>
      </template>
      <p v-else class="update-panel-empty">
        暂无待处理更新。
      </p>
    </div>
  </section>

  <DispatchNotifyModal :is-open="dispatchNotifyOpen" :flight-no-resolver="flightNoResolver" @close="emit('close-dispatch')" />
  <CriticalNotifyModal
    :notification-queue="criticalNotificationQueue"
    :pop-notification="() => { const n = criticalNotificationQueue[0]; emit('shift-critical-notification'); return n; }"
  />
  <DispatchReminderModal
    :queue="sentReceiptReminderQueue"
    :pop-reminder="() => { const id = sentReceiptReminderQueue[0]; emit('shift-reminder'); return id; }"
    @view-history="emit('view-reminder-history')"
  />
  <FlightInsightModal
    :is-open="flightInsightOpen"
    :flight-id="selectedFlightId"
    :flight-no="selectedFlightNo"
    @close="emit('close-insight')"
  />

  <AIAssistantFloatPanel
    v-model:open="aiAssistantOpen"
    :selected-flight-id="selectedFlightId"
    :selected-flight-no="selectedFlightNo"
    @update:unread="aiUnread = $event"
  />

  <AutoCopilotVoicePanel
    v-model:open="copilotOpen"
    :selected-flight-id="selectedFlightId"
    :selected-flight-no="selectedFlightNo"
    :business-case-types="businessCaseTypes"
    @created="emit('auto-copilot-created', $event)"
  />

  <DispatchCollaborationChat
    :is-open="dispatchChatOpen"
    :flight-id="selectedFlightId"
    @close="emit('close-chat')"
    @toast="emit('toast', $event)"
    @error="emit('error', $event)"
  />
</template>

<style scoped>
/* 角浮悬钮：一页一颗，主声实底；hover 有位移 */
.fm-dock-fab {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border: none;
  border-radius: var(--r-control);
  background: var(--act);
  color: var(--act-on);
  cursor: pointer;
  box-shadow: var(--shadow-md);
  transition: transform var(--t-fast) var(--ease);
}

.fm-dock-fab:hover {
  transform: translateY(-1px);
}

.fm-dock-fab:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}

.fm-dock-fab[aria-expanded='true'] {
  box-shadow: var(--shadow-md), inset 0 0 0 2px color-mix(in srgb, var(--act-on) 35%, transparent);
}

.fm-dock-fab-badge {
  position: absolute;
  top: -6px;
  right: -6px;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: var(--r-pill);
  background: var(--danger);
  color: var(--danger-on);
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

/* 抬起面板：临时工具箱，菜单条目列 */
.fm-dock-panel {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 176px;
  padding: 4px;
  background: var(--face-raised);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  animation: fm-dock-pop var(--t-mid) var(--ease);
}

@keyframes fm-dock-pop {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.update-panel {
  position: fixed;
  bottom: 108px;
  right: 20px;
  width: min(360px, calc(100vw - 40px));
  background: var(--face-raised);
  border: 1px solid var(--line);
  border-radius: var(--r-panel);
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
  z-index: 9100;
  overflow: hidden;
}

.update-panel-header {
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.update-panel-title {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.update-panel-close {
  background: none;
  border: none;
  color: var(--ink-subtle);
  font-size: 20px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
}

.update-panel-close:hover {
  color: var(--ink);
}

.update-panel-content {
  padding: 12px 16px;
  font-size: var(--fs-body);
  color: var(--ink);
  max-height: 340px;
  overflow-y: auto;
}

.update-panel-message {
  margin: 0 0 10px;
  text-align: left;
}

.update-panel-empty {
  margin: 0;
  color: var(--ink-muted);
  text-align: center;
  padding: 8px 0;
}
</style>
