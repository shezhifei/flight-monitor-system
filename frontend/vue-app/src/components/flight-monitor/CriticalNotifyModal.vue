<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useNotification } from '@/composables/useNotification';
import type { UserNotification } from '@/composables/useFlightStream';
import UiModal from '../ui/UiModal.vue';
import UiButton from '../ui/UiButton.vue';
import UiField from '../ui/UiField.vue';
import UiInset from '../ui/UiInset.vue';
import UiPill from '../ui/UiPill.vue';

/**
 * 关键通知：必须回执才能继续。弹窗 closable 关掉、脚上两颗谓词已经把这句话说完了
 * （§3.8 / §4.4 不要加教学小字）。
 *
 * 正文用嵌板降到页底，身里不再描第二道边、不再铺抬起面（§3.8 / §3.7）。
 * 事态一颗胶囊，来源次墨小字排在时刻旁边（§2.5 / §4.4）。
 */
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

/** 事态画在对象上用胶囊（§2.5）；声只有四声（§2.4），枚举原样不丢给值班的人。 */
function severityTone(s: string): 'warn' | 'danger' | 'mute' {
  const v = s.trim().toLowerCase();
  if (v === 'critical') return 'danger';
  if (v === 'high' || v === 'urgent') return 'danger';
  if (v === 'warning' || v === 'medium') return 'warn';
  return 'mute';
}

function severityLabel(s: string): string {
  const v = s.trim().toLowerCase();
  if (v === 'critical') return '关键';
  if (v === 'high' || v === 'urgent') return '紧急';
  if (v === 'warning' || v === 'medium') return '警示';
  return s.trim();
}

/** 来源不是事态，不另开胶囊（§4.4）。 */
function originLabel(s: string): string {
  const v = s.trim().toLowerCase();
  if (v === 'system') return '系统';
  if (v === 'dispatch') return '调度';
  if (v === 'manual') return '人工';
  return s.trim();
}

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
      <div class="notification-meta">
        <span class="notification-time">{{ activeNotification.timestamp }}</span>
        <span class="notification-origin">{{ originLabel(String(activeNotification.origin_type || 'SYSTEM')) }}</span>
        <UiPill :tone="severityTone(String(activeNotification.severity || 'CRITICAL'))">
          {{ severityLabel(String(activeNotification.severity || 'CRITICAL')) }}
        </UiPill>
        <span v-if="relatedFlightText" class="flight-related">航班 {{ relatedFlightText }}</span>
      </div>
      <div class="notification-title">
        {{ activeNotification.title || '系统安全通知' }}
      </div>
      <!-- 身自带内衬；正文降一级到嵌板，不再铺抬起面（§3.8 / §3.7）。 -->
      <UiInset class="notification-inset">
        <div class="notification-body">
          {{ activeNotification.body || '暂无正文内容...' }}
        </div>
      </UiInset>
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
.notification-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  font-size: var(--fs-label);
  color: var(--ink-subtle);
  margin-bottom: 8px;
}

/* 时刻用等宽（§2.4）。 */
.notification-time {
  font-family: var(--mono);
}

.notification-origin {
  color: var(--ink-subtle);
}

/* 航班号是标识，用等宽主墨，不用行动蓝冒充强调（§2.4）。 */
.flight-related {
  color: var(--ink);
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}

.notification-title {
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
  margin-bottom: 12px;
}

.notification-inset {
  margin-bottom: 16px;
}

/* 形交给嵌板；这里只留正文自己的排版（§3.7 降到页底，不是升到工作面）。 */
.notification-body {
  font-size: var(--fs-section);
  line-height: 1.6;
  color: var(--ink);
  white-space: pre-wrap;
}
</style>
