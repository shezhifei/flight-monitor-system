<script setup lang="ts">
import { computed } from 'vue';
import {
  formatSharedPersonnel,
  type DispatchOverrunWarning,
} from '@/composables/useDispatchOverrunWarnings';

const props = defineProps<{
  warnings: readonly DispatchOverrunWarning[];
  busyIds?: ReadonlySet<string> | Set<string>;
}>();

const emit = defineEmits<{
  (e: 'acknowledge', id: string): void;
  (e: 'resolve', id: string): void;
  (e: 'jump-order', orderId: string): void;
  (e: 'jump-orders', payload: { currentOrderId?: string | null; nextOrderId?: string | null }): void;
}>();

const visible = computed(() =>
  (props.warnings || []).filter((w) => !w.is_resolved),
);

function isBusy(id: string): boolean {
  return Boolean(props.busyIds?.has(id));
}

function sharedLabel(warning: DispatchOverrunWarning): string {
  return formatSharedPersonnel(warning.details?.shared_personnel);
}

function countdownLabel(warning: DispatchOverrunWarning): string {
  const minutes = warning.details?.countdown_minutes;
  if (typeof minutes === 'number' && Number.isFinite(minutes)) {
    if (minutes <= 0) {
      return '已到计划开始';
    }
    return `距下一单 ${minutes} 分钟`;
  }
  return '';
}

function conflictLabel(warning: DispatchOverrunWarning): string {
  if (warning.details?.eta_missing) {
    return '未回报预计完成时间';
  }
  const conflict = warning.details?.predicted_conflict_minutes;
  if (typeof conflict === 'number' && Number.isFinite(conflict)) {
    return conflict > 0 ? `预计冲突 ${conflict} 分钟` : '预计不冲突';
  }
  return '';
}

function onJump(warning: DispatchOverrunWarning): void {
  emit('jump-orders', {
    currentOrderId: warning.current_order_id,
    nextOrderId: warning.next_order_id,
  });
  const target = warning.current_order_id || warning.next_order_id;
  if (target) {
    emit('jump-order', target);
  }
}
</script>

<template>
  <section
    v-if="visible.length > 0"
    class="overrun-warning-bar"
    aria-label="预排冲突预警"
    role="region"
  >
    <header class="overrun-warning-bar__head">
      <span class="overrun-warning-bar__badge" aria-hidden="true">⚠</span>
      <strong>预排冲突预警</strong>
      <span class="overrun-warning-bar__count">{{ visible.length }}</span>
      <span class="overrun-warning-bar__hint">不阻断派工 / 发布 / 重排</span>
    </header>

    <ul class="overrun-warning-bar__list">
      <li
        v-for="warning in visible"
        :key="warning.dedupe_key || warning.id"
        class="overrun-warning-item"
        :data-severity="warning.severity || 'warning'"
        :data-acknowledged="warning.acknowledged_at ? 'true' : 'false'"
      >
        <div class="overrun-warning-item__body">
          <div class="overrun-warning-item__message">
            {{ warning.message || '共享人员可能影响下一工单' }}
            <span
              v-if="warning.acknowledged_at"
              class="overrun-warning-item__seen"
            >已确认</span>
          </div>
          <div class="overrun-warning-item__meta">
            <span v-if="warning.flight_id" class="meta-chip">航班 {{ warning.flight_id }}</span>
            <button
              v-if="warning.current_order_id"
              type="button"
              class="meta-link"
              @click="emit('jump-order', warning.current_order_id!)"
            >
              当前单 {{ warning.current_order_id }}
            </button>
            <button
              v-if="warning.next_order_id"
              type="button"
              class="meta-link"
              @click="emit('jump-order', warning.next_order_id!)"
            >
              下一单 {{ warning.next_order_id }}
            </button>
            <span v-if="sharedLabel(warning)" class="meta-chip">
              共享人员 {{ sharedLabel(warning) }}
            </span>
            <span v-if="countdownLabel(warning)" class="meta-chip is-countdown">
              {{ countdownLabel(warning) }}
            </span>
            <span
              v-if="conflictLabel(warning)"
              class="meta-chip"
              :class="warning.details?.eta_missing ? 'is-eta-missing' : 'is-conflict'"
            >
              {{ conflictLabel(warning) }}
            </span>
            <span
              v-if="warning.occurrence_count && warning.occurrence_count > 1"
              class="meta-chip"
            >
              第 {{ warning.occurrence_count }} 次
            </span>
          </div>
        </div>
        <div class="overrun-warning-item__actions">
          <button
            type="button"
            class="overrun-btn"
            :disabled="isBusy(warning.id) || Boolean(warning.acknowledged_at)"
            :title="warning.acknowledged_at ? '已确认（仅表示已看过）' : '确认已看到（不关闭）'"
            @click="emit('acknowledge', warning.id)"
          >
            确认
          </button>
          <button
            type="button"
            class="overrun-btn is-primary"
            :disabled="isBusy(warning.id)"
            title="关闭该预警"
            @click="emit('resolve', warning.id)"
          >
            关闭
          </button>
          <button
            type="button"
            class="overrun-btn is-ghost"
            title="跳转到相关工单"
            @click="onJump(warning)"
          >
            定位工单
          </button>
        </div>
      </li>
    </ul>
  </section>
</template>
