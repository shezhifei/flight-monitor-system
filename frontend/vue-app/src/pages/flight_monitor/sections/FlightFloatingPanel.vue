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
import UiFab from '../../../components/ui/UiFab.vue';
import UiFloatPanel from '../../../components/ui/UiFloatPanel.vue';
import UiMenu from '../../../components/ui/UiMenu.vue';
import type { UserNotification } from '@/types/bindings';
import type { BusinessCaseTypeDefinition } from '../../../types/backend';
import type { ChatNotificationTarget } from '../../../composables/chatTargetFromNotification';

const props = defineProps<{
  anomalyCount: number;
  anomalySeverity: 'high' | 'medium' | 'low';
  /** 异常池开着 = 持守，和「待处理更新」同一种形（§2.5） */
  alertPoolOpen: boolean;
  updateMessages: string[];
  updatePanelOpen: boolean;
  notificationCount: number;
  dispatchNotifyOpen: boolean;
  dispatchChatOpen: boolean;
  flightInsightOpen: boolean;
  selectedFlightId: string | null;
  chatGroupId?: string | null;
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
  (e: 'open-chat-from-notification', target: ChatNotificationTarget): void;
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
const chatUnread = ref(0);

// 角浮：一页一颗主声悬钮，点开抬起临时工具箱
const dockOpen = ref(false);
const dockRoot = ref<HTMLElement | null>(null);

/**
 * 悬钮收起时只报一个数：先报最急的那一件，声跟着它。
 * 未读的 AI 回话不是危，异常池的轻重按 anomalySeverity 走。
 * 工具箱展开后每一条自己报数，这颗徽记就退场（§4.4 不要重复芯片）。
 */
const fabSignal = computed(() => {
  if (props.anomalyCount > 0) {
    return {
      count: props.anomalyCount,
      tone: props.anomalySeverity === 'high' ? 'danger' : 'warn',
    } as const;
  }
  if (props.notificationCount > 0) return { count: props.notificationCount, tone: 'danger' } as const;
  if (chatUnread.value > 0) return { count: chatUnread.value, tone: 'mute' } as const;
  if (aiUnread.value > 0) return { count: aiUnread.value, tone: 'mute' } as const;
  return null;
});

/** 一次性的入口点完就把工具箱收起来；持守那几条留着，好让人看见形变了。 */
function pick(run: () => void): void {
  dockOpen.value = false;
  run();
}

function onDocPointerDown(event: PointerEvent): void {
  if (dockRoot.value && !dockRoot.value.contains(event.target as Node)) dockOpen.value = false;
}

onMounted(() => document.addEventListener('pointerdown', onDocPointerDown));
onBeforeUnmount(() => document.removeEventListener('pointerdown', onDocPointerDown));
</script>

<template>
  <div ref="dockRoot">
    <UiDock label="临时工具箱">
      <UiMenu
        v-if="dockOpen"
        label="临时工具箱"
        min-width="176px"
      >
        <UiDockButton
          v-if="anomalyCount > 0"
          label="异常告警"
          :count="anomalyCount"
          :tone="anomalySeverity === 'high' ? 'danger' : 'warn'"
          :pressed="alertPoolOpen"
          @click="emit('toggle-anomaly-pool')"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M12 3 2.5 19.5h19L12 3z" /><path d="M12 10v4" /><path d="M12 17v.5" /></svg>
        </UiDockButton>
        <UiDockButton
          label="待处理更新"
          :count="updateMessages.length"
          :pressed="updatePanelOpen"
          @click="emit('toggle-update-panel')"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M20 12a8 8 0 1 1-2.34-5.66" /><path d="M20 4v4h-4" /></svg>
        </UiDockButton>
        <UiDockButton
          label="调度网关"
          :count="notificationCount"
          :tone="notificationCount > 0 ? 'danger' : 'mute'"
          @click="pick(() => emit('open-dispatch'))"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M6 9a6 6 0 1 1 12 0c0 5 2 6 2 6H4s2-1 2-6" /><path d="M10 19a2 2 0 0 0 4 0" /></svg>
        </UiDockButton>
        <UiDockButton label="AI 洞察" :count="null" @click="pick(() => emit('open-insight'))">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z" /></svg>
        </UiDockButton>
        <UiDockButton
          label="协同群聊"
          :count="chatUnread > 0 ? chatUnread : null"
          :tone="chatUnread > 0 ? 'act' : 'mute'"
          @click="pick(() => emit('open-chat'))"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M4 5h16v11H9l-5 4z" /></svg>
        </UiDockButton>
        <UiDockButton
          label="极智 AI 指挥官"
          :count="aiUnread > 0 ? aiUnread : null"
          :pressed="aiAssistantOpen"
          @click="aiAssistantOpen = !aiAssistantOpen"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6z" /></svg>
        </UiDockButton>
        <UiDockButton label="语音事项" :pressed="copilotOpen" @click="copilotOpen = !copilotOpen">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          ><path d="M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z" /><path d="M6 11a6 6 0 0 0 12 0" /><path d="M12 17v4" /></svg>
        </UiDockButton>
      </UiMenu>
      <UiFab
        class="fm-dock-fab"
        label="临时工具箱"
        haspopup
        :expanded="dockOpen"
        :count="!dockOpen && fabSignal ? fabSignal.count : null"
        :tone="fabSignal?.tone ?? 'mute'"
        @click="dockOpen = !dockOpen"
      >
        <svg
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        ><path d="M4 4h7v7H4z" /><path d="M13 4h7v7h-7z" /><path d="M4 13h7v7H4z" /><path d="M13 13h7v7h-7z" /></svg>
      </UiFab>
    </UiDock>
  </div>

  <!-- 更新面板就是一只浮舱：面 / 帽 / 身归 UiFloatPanel，落点用坞那一组 token -->
  <UiFloatPanel
    :open="updatePanelOpen"
    title="更新面板"
    width="min(360px, calc(100vw - 40px))"
    height="min(420px, calc(100vh - 168px))"
    @close="emit('close-update-panel')"
  >
    <div class="update__list">
      <p v-for="message in updateMessages" :key="message" class="update__msg">
        {{ message }}
      </p>
      <p v-if="!updateMessages.length" class="update__void">
        暂无待处理更新
      </p>
    </div>
  </UiFloatPanel>

  <DispatchNotifyModal
    :is-open="dispatchNotifyOpen"
    :flight-no-resolver="flightNoResolver"
    @close="emit('close-dispatch')"
    @open-chat="emit('open-chat-from-notification', $event)"
  />
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
    :flight-id="chatGroupId ? null : selectedFlightId"
    :group-id="chatGroupId || undefined"
    @close="emit('close-chat')"
    @toast="emit('toast', $event)"
    @error="emit('error', $event)"
    @unread="chatUnread = $event"
  />
</template>

<style scoped>
/* 形归 UiFab；这一页的坞钮保持小一号、方一点的形 */
.fm-dock-fab {
  width: var(--h-lg);
  height: var(--h-lg);
  border-radius: var(--r-control);
}

/* 工具箱那一列的形归 UiMenu（§3.6）；这里只给它在坞里的落点 */
/* 浮舱的面、帽、关都在 UiFloatPanel 里；这里只剩身里那一列的排布 */
.update__list {
  padding: var(--s1) 0;
}

.update__msg {
  margin: 0;
  padding: var(--s2) var(--s3);
  color: var(--ink);
  line-height: 1.5;
}

.update__msg + .update__msg {
  border-top: 1px solid var(--line);
}

.update__void {
  margin: 0;
  padding: var(--s5) var(--s3);
  color: var(--ink-muted);
  text-align: center;
}
</style>
