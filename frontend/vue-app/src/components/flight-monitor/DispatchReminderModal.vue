<template>
  <transition name="modal-fade">
    <div
      v-if="activeReminder"
      class="modal-overlay"
      style="z-index: 10000;"
      @mousedown.self="close"
      @click.self="close"
    >
      <div
        class="modal-container"
        style="max-width: 480px;"
        @mousedown.stop
        @click.stop
      >
        <div class="modal-header" style="background: linear-gradient(135deg, #b45309 0%, #92400e 100%); padding: 16px 20px; margin-bottom: 0;">
          <h3 style="margin:0;font-size:18px;color:#fff;">
            回执超时提醒
          </h3>
          <button
            type="button"
            class="close-modal"
            aria-label="关闭提醒"
            style="color:#fff;"
            @click="close"
          >
            ×
          </button>
        </div>

        <div class="modal-body" style="padding: 20px;">
          <div style="font-size: 15px; color: var(--admin-text); margin-bottom: 16px; line-height: 1.6;">
            您发出的调度指令仍有人员未确认，指令流转由于超时状态可能存在安全隐患，请及时跟进处理。
          </div>
          
          <div style="background: var(--bg-page); border-radius: 8px; padding: 16px; border: 1px solid var(--admin-border); margin-bottom: 24px;">
            <div style="display: flex; margin-bottom: 8px;">
              <div style="width: 80px; color: var(--admin-text-subtle); font-size: 13px;">
                通知标题
              </div>
              <div style="flex: 1; color: var(--admin-text); font-size: 14px; font-weight: 500;">
                {{ activeReminder.title || '未命名通知' }}
              </div>
            </div>
            <div style="display: flex; margin-bottom: 8px;">
              <div style="width: 80px; color: var(--admin-text-subtle); font-size: 13px;">
                回执状态
              </div>
              <div style="flex: 1; color: #b45309; font-size: 14px; font-weight: 500;">
                已回复 {{ activeReminder.total_count - activeReminder.pending_count }} 人，待回复 {{ activeReminder.pending_count }} 人
              </div>
            </div>
            <div style="display: flex;">
              <div style="width: 80px; color: var(--admin-text-subtle); font-size: 13px;">
                关联航班
              </div>
              <div style="flex: 1; color: var(--admin-text); font-size: 14px;">
                {{ activeReminder.flight_id || '无' }}
              </div>
            </div>
          </div>
        </div>

        <div class="modal-footer" style="padding: 16px 20px; border-top: 1px solid var(--admin-border); display: flex; justify-content: flex-end; gap: 12px; background: var(--bg-card); border-bottom-left-radius: 12px; border-bottom-right-radius: 12px;">
          <button type="button" class="flight-btn flight-btn-secondary" @click="close">
            稍后处理
          </button>
          <button type="button" class="flight-btn flight-btn-primary" @click="viewDetail">
            查看回执详情
          </button>
        </div>
      </div>
    </div>
  </transition>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { useNotification } from '@/composables/useNotification';
import type { SentReceiptGroupSummaryResponse } from '@/composables/useNotification';

const props = defineProps<{
  queue: string[];
  popReminder: () => string | undefined;
}>();

const notificationAPI = useNotification();
const activeReminder = ref<SentReceiptGroupSummaryResponse | null>(null);

const rootEmit = defineEmits<{
  (e: 'view-history', groupId: string): void
}>();

async function pollQueue() {
  if (!activeReminder.value && props.queue.length > 0) {
    const groupId = props.popReminder();
    if (groupId) {
      // Fetch details of this group to check if STILL pending
      const detail = await notificationAPI.fetchHistoryDetail(groupId);
      if (detail && detail.summary.pending_count > 0 && detail.summary.is_overdue) {
        activeReminder.value = {
           receipt_group_id: detail.receipt_group_id,
           title: detail.title,
           severity: detail.severity,
           origin_type: detail.origin_type,
           origin_label: detail.origin_label,
           flight_id: detail.flight_id,
           is_overdue: detail.summary.is_overdue,
           total_count: detail.summary.total_count,
           pending_count: detail.summary.pending_count,
           acknowledged_count: detail.summary.acknowledged_count,
           rejected_count: detail.summary.rejected_count,
           remind_after_at: detail.summary.remind_after_at,
           latest_updated_at: detail.summary.latest_updated_at
        };
      } else {
        pollQueue(); // Try next one
      }
    }
  }
}

watch(() => props.queue.length, () => {
  pollQueue();
});

onMounted(() => {
  pollQueue();
});

function close() {
  activeReminder.value = null;
  pollQueue();
}

function viewDetail() {
  if (activeReminder.value) {
    rootEmit('view-history', activeReminder.value.receipt_group_id);
    close();
  }
}
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background-color: rgba(15, 23, 42, 0.6);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
}

.modal-container {
  width: 100%;
  background: var(--admin-card-bg);
  border-radius: 12px;
  box-shadow: 0 20px 40px -10px rgba(0, 0, 0, 0.3);
  display: flex;
  flex-direction: column;
  animation: modalPop 0.25s ease-out forwards;
}

@keyframes modalPop {
  0% { transform: scale(0.97); opacity: 0; }
  100% { transform: scale(1); opacity: 1; }
}
</style>
