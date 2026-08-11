<template>
  <transition name="modal-fade">
    <div
      v-if="activeNotification"
      class="modal-overlay blocking-overlay"
      style="z-index: 10001;"
      @mousedown.stop
      @click.stop
      @keydown.prevent.stop
    >
      <div class="modal-container critical-dialog">
        <div class="modal-header critical-header">
          <div class="critical-title-wrap">
            <h3 style="margin:0;font-size:20px;color:#f1f5f9;">
              关键通知待处理
            </h3>
            <div class="critical-meta" style="margin-top:6px;font-size:13px;color:rgba(255,255,255,0.88);">
              此强制拦截通知必须确认收到或拒绝后系统才能解除阻断并继续操作
            </div>
          </div>
          <span class="critical-badge">
            <span class="critical-dot" />CRITICAL
          </span>
        </div>

        <div class="modal-body critical-body">
          <div class="notification-insight">
            <div class="notification-meta" style="font-size:12px;color:var(--admin-text-subtle);margin-bottom:8px;">
              {{ activeNotification.timestamp }} | 级别：{{ (activeNotification.severity || 'CRITICAL').toUpperCase() }} | 发信来源：{{ activeNotification.origin_type || 'SYSTEM' }}
              <span v-if="relatedFlightText" class="flight-related" style="margin-left: 8px; color: #3b82f6;">航班 {{ relatedFlightText }}</span>
            </div>
            <div class="notification-title" style="font-size:18px;font-weight:600;color:var(--admin-text);margin-bottom:12px;">
              {{ activeNotification.title || '系统安全通知' }}
            </div>
            <div class="notification-body" style="font-size:15px;line-height:1.6;color:var(--admin-text);white-space:pre-wrap;background:var(--bg-page);padding:16px;border-radius:12px;border:1px solid var(--admin-border);margin-bottom:20px;">
              {{ activeNotification.body || '暂无正文内容...' }}
            </div>
            
            <div class="reject-reason-section">
              <label style="font-size:13px;font-weight:600;color:var(--admin-text);display:block;margin-bottom:8px;">拒绝原因（若拒绝执行则必填）</label>
              <textarea 
                v-model="rejectNote" 
                class="premium-textarea reject-input" 
                placeholder="请输入拒绝原因。若是确认收到并能够执行，此项留空即可"
                rows="4"
                style="width:100%;border:1px solid rgba(148,163,184,0.4);border-radius:12px;padding:12px;font-size:14px;resize:vertical;"
              />
              <div v-if="errorMsg" class="error-text" style="font-size:13px;color:#b91c1c;margin-top:8px;">
                {{ errorMsg }}
              </div>
            </div>
          </div>
        </div>

        <div class="modal-footer critical-footer" style="padding:16px 24px;border-top:1px solid var(--admin-border);display:flex;justify-content:flex-end;gap:12px;background:var(--bg-page);border-bottom-left-radius:16px;border-bottom-right-radius:16px;">
          <button
            type="button"
            class="btn btn-danger"
            :disabled="isSubmitting"
            style="padding:10px 20px;border-radius:8px;font-weight:600;background:#fef2f2;color:#ef4444;border:1px solid rgba(239, 68, 68, 0.3);cursor:pointer;transition:all 0.2s;"
            @click="handleAck('rejected')"
          >
            {{ isSubmitting && submittingAction === 'rejected' ? '提交中...' : '无法执行并拒绝' }}
          </button>
          <button
            type="button"
            class="btn btn-primary"
            :disabled="isSubmitting"
            style="padding:10px 20px;border-radius:8px;font-weight:600;background:var(--admin-text);color:#fff;border:none;cursor:pointer;transition:all 0.2s;"
            @click="handleAck('acknowledged')"
          >
            {{ isSubmitting && submittingAction === 'acknowledged' ? '提交中...' : '确认收到 (ACK)' }}
          </button>
        </div>
      </div>
    </div>
  </transition>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useNotification } from '@/composables/useNotification';
import type { UserNotification } from '@/composables/useFlightStream';

const props = defineProps<{
  notificationQueue: UserNotification[];
  popNotification: () => UserNotification | undefined;
}>();

const notificationAPI = useNotification();

const rejectNote = ref('');
const errorMsg = ref('');
const isSubmitting = ref(false);
const submittingAction = ref<'acknowledged' | 'rejected' | null>(null);

// Currently displaying notification
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
      // Clear current, let it poll the next one if it exists
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

<style scoped>
.blocking-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background-color: rgba(15, 23, 42, 0.85);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
}

.critical-dialog {
  width: 100%;
  max-width: 540px;
  background: var(--admin-card-bg);
  border-radius: 16px;
  box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--error-border-subtle);
  animation: modalPop 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275) forwards;
}

.critical-header {
  background: linear-gradient(135deg, #b91c1c 0%, #991b1b 100%);
  padding: 20px 24px;
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  flex-shrink: 0;
  /* 覆盖全局 layout.css .modal-header 的 margin-bottom: 1.25rem */
  margin-bottom: 0;
}

.critical-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: rgba(255, 255, 255, 0.2);
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  color: var(--text-inverse);
  letter-spacing: 0.05em;
  border: 1px solid rgba(255, 255, 255, 0.3);
}

.critical-dot {
  width: 6px; height: 6px;
  border-radius: 50%;
  background-color: #fca5a5;
  box-shadow: 0 0 8px #fca5a5;
  animation: pulse 1.5s infinite;
}

.critical-body {
  padding: 24px;
  overflow-y: auto;
  max-height: 60vh;
}

@keyframes modalPop {
  0% { transform: scale(0.95); opacity: 0; }
  100% { transform: scale(1); opacity: 1; }
}

@keyframes pulse {
  0% { opacity: 1; box-shadow: 0 0 0 0 rgba(252, 165, 165, 0.7); }
  70% { opacity: 0.5; box-shadow: 0 0 0 6px rgba(252, 165, 165, 0); }
  100% { opacity: 1; box-shadow: 0 0 0 0 rgba(252, 165, 165, 0); }
}

.btn-primary:hover:not(:disabled) { background: #1e293b; }
.btn-danger:hover:not(:disabled) { background: var(--dh-signal-critical-soft); border-color: var(--system-red); }
.btn:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
