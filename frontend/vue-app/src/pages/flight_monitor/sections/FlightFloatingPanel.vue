<script setup lang="ts">
import FloatingBadges from '../../../components/flight-monitor/FloatingBadges.vue';
import AIAssistantFloatPanel from '../../../components/flight-monitor/AIAssistantFloatPanel.vue';
import AutoCopilotVoicePanel from '../../../components/flight-monitor/AutoCopilotVoicePanel.vue';
import DispatchCollaborationChat from '../../../components/flight-monitor/DispatchCollaborationChat.vue';
import DispatchNotifyModal from '../../../components/flight-monitor/DispatchNotifyModal.vue';
import CriticalNotifyModal from '../../../components/flight-monitor/CriticalNotifyModal.vue';
import DispatchReminderModal from '../../../components/flight-monitor/DispatchReminderModal.vue';
import FlightInsightModal from '../../../components/flight-monitor/FlightInsightModal.vue';
import type { UserNotification } from '@/types/bindings';
import type { BusinessCaseTypeDefinition } from '../../../types/backend';

defineProps<{
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
</script>

<template>
  <FloatingBadges
    :anomaly-count="anomalyCount"
    :anomaly-severity="anomalySeverity"
    :update-messages="updateMessages"
    :update-panel-open="updatePanelOpen"
    :notification-count="notificationCount"
    @toggle-anomaly-pool="emit('toggle-anomaly-pool')"
    @toggle-update-panel="emit('toggle-update-panel')"
    @close-update-panel="emit('close-update-panel')"
    @open-dispatch="emit('open-dispatch')"
    @open-chat="emit('open-chat')"
    @open-insight="emit('open-insight')"
  />

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
    :selected-flight-id="selectedFlightId"
    :selected-flight-no="selectedFlightNo"
  />

  <AutoCopilotVoicePanel
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
