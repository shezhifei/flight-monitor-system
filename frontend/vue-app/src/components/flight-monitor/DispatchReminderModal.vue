<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { useNotification } from '@/composables/useNotification';
import type { SentReceiptGroupSummaryResponse } from '@/composables/useNotification';
import UiModal from '../ui/UiModal.vue';
import UiButton from '../ui/UiButton.vue';

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
        pollQueue();
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

<template>
  <UiModal
    :open="Boolean(activeReminder)"
    title="回执超时"
    :width="520"
    @close="close"
  >
    <template v-if="activeReminder">
      <p class="lede">
        发出的调度指令仍有人员未确认。
      </p>
      <dl class="meta">
        <div class="meta-row">
          <dt>通知标题</dt>
          <dd>{{ activeReminder.title || '未命名通知' }}</dd>
        </div>
        <div class="meta-row">
          <dt>回执状态</dt>
          <dd class="meta-warn">
            已回复 {{ activeReminder.total_count - activeReminder.pending_count }} 人，待回复 {{ activeReminder.pending_count }} 人
          </dd>
        </div>
        <div class="meta-row">
          <dt>关联航班</dt>
          <dd>{{ activeReminder.flight_id || '无' }}</dd>
        </div>
      </dl>
    </template>
    <template #footer>
      <UiButton variant="ghost" @click="close">稍后处理</UiButton>
      <UiButton variant="primary" @click="viewDetail">查看回执详情</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.lede {
  margin: 0 0 16px;
  font-size: var(--fs-section);
  color: var(--ink);
  line-height: 1.6;
}

.meta {
  margin: 0;
  background: var(--face-work);
  border-radius: var(--r-control);
  padding: 12px 14px;
  border: 1px solid var(--line);
}

.meta-row {
  display: flex;
  margin-bottom: 8px;
}

.meta-row:last-child {
  margin-bottom: 0;
}

.meta-row dt {
  width: 80px;
  flex-shrink: 0;
  color: var(--ink-subtle);
  font-size: var(--fs-body);
}

.meta-row dd {
  margin: 0;
  flex: 1;
  color: var(--ink);
  font-size: var(--fs-section);
}

.meta-warn {
  color: var(--warn);
  font-variant-numeric: tabular-nums;
}
</style>
