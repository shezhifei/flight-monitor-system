<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useNotification } from '@/composables/useNotification';
import type { UserNotification } from '@/composables/useFlightStream';
import UiModal from '../ui/UiModal.vue';
import UiButton from '../ui/UiButton.vue';
import UiField from '../ui/UiField.vue';

const props = defineProps<{
  notificationQueue: UserNotification[];
  popNotification: () => UserNotification | undefined;
}>();

const notificationAPI = useNotification();

const rejectNote = ref('');
const errorMsg = ref('');
const isSubmitting = ref(false);
const submittingAction = ref<'acknowledged' | 'rejected' | null>(null);
const activeNotification = ref<UserNotification | null>(null);

const relatedFlightText = computed(() => {
  if (!activeNotification.value) return '';
  return activeNotification.value.related_flight_label || activeNotification.value.related_flight_no || activeNotification.value.flight_no || activeNotification.value.flight_id || '';
});

function pollQueue() {
  if (!activeNotification.value && props.notificationQueue.length > 0) {
    const next = props.popNotification();
    if (next) {
      activeNotification.value = next;
      rejectNote.value = '';
      errorMsg.value = '';
    }
  }
}

watch(() => props.notificationQueue.length, () => {
  pollQueue();
});

onMounted(() => {
  pollQueue();
});

async function handleAck(action: 'acknowledged' | 'rejected') {
  if (!activeNotification.value) return;
  const note = rejectNote.value.trim();

  if (action === 'rejected' && !note) {
    errorMsg.value = '必须填写拒绝原因，系统强制要求。';
    return;
  }

  errorMsg.value = '';
  isSubmitting.value = true;
  submittingAction.value = action;

  try {
    const success = await notificationAPI.acknowledge(String(activeNotification.value.notification_id), action, note);
    if (!success) {
      errorMsg.value = '提交回执失败，请重试';
    } else {
      activeNotification.value = null;
      pollQueue();
    }
  } catch (err: unknown) {
    errorMsg.value = (err as { message?: string })?.message || '提交回执出错';
  } finally {
    isSubmitting.value = false;
    submittingAction.value = null;
  }
}
</script>

<template>
  <UiModal
    :open="Boolean(activeNotification)"
    title="关键通知"
    :width="560"
    :closable="false"
  >
    <template v-if="activeNotification">
      <p class="critical-lede">
        此通知必须确认收到或拒绝后才能继续操作。
      </p>
      <div class="notification-meta">
        {{ activeNotification.timestamp }} · {{ (activeNotification.severity || 'CRITICAL').toUpperCase() }} · {{ activeNotification.origin_type || 'SYSTEM' }}
        <span v-if="relatedFlightText" class="flight-related">航班 {{ relatedFlightText }}</span>
      </div>
      <div class="notification-title">
        {{ activeNotification.title || '系统安全通知' }}
      </div>
      <div class="notification-body">
        {{ activeNotification.body || '暂无正文内容...' }}
      </div>
      <UiField label="拒绝原因（若拒绝执行则必填）" for-id="criticalRejectNote" :error="errorMsg">
        <textarea
          id="criticalRejectNote"
          v-model="rejectNote"
          placeholder="请输入拒绝原因。若是确认收到并能够执行，此项留空即可"
          rows="4"
        />
      </UiField>
    </template>
    <template #footer>
      <UiButton variant="danger" :disabled="isSubmitting" @click="handleAck('rejected')">
        {{ isSubmitting && submittingAction === 'rejected' ? '提交中...' : '无法执行并拒绝' }}
      </UiButton>
      <UiButton variant="primary" :disabled="isSubmitting" @click="handleAck('acknowledged')">
        {{ isSubmitting && submittingAction === 'acknowledged' ? '提交中...' : '确认收到' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.critical-lede {
  margin: 0 0 12px;
  font-size: var(--fs-body);
  color: var(--ink-subtle);
  line-height: 1.5;
}

.notification-meta {
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  margin-bottom: 8px;
}

.flight-related {
  margin-left: 8px;
  color: var(--act);
  font-variant-numeric: tabular-nums;
}

.notification-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin-bottom: 12px;
}

.notification-body {
  font-size: var(--fs-section);
  line-height: 1.6;
  color: var(--ink);
  white-space: pre-wrap;
  background: var(--face-work);
  padding: 12px 14px;
  border-radius: var(--r-control);
  border: 1px solid var(--line);
  margin-bottom: 16px;
}

.reject-label {
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  display: block;
  margin-bottom: 8px;
}

.reject-input {
  width: 100%;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  padding: 10px 12px;
  font-size: var(--fs-section);
  resize: vertical;
  background: var(--face-work);
  color: var(--ink);
  box-sizing: border-box;
}

.reject-input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.error-text {
  font-size: var(--fs-body);
  color: var(--danger);
  margin-top: 8px;
}

.btn-reject,
.btn-ack {
  padding: 0 16px;
  height: var(--h-md);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  cursor: pointer;
}

.btn-reject {
  background: var(--danger-soft);
  color: var(--danger);
  border: 1px solid var(--danger);
}

.btn-ack {
  background: var(--act);
  color: var(--act-on);
  border: 1px solid var(--act);
}

.btn-reject:disabled,
.btn-ack:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-reject:focus-visible,
.btn-ack:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 2px;
}
</style>
