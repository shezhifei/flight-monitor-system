<script setup lang="ts">
import type { BusinessCaseTypeDefinition } from '../../../types/backend';
import type { BoundFlightBindingOption } from '../composables/useFlightMonitorModals';

defineProps<{
  isOpen: boolean;
  eventType: string;
  eventStatus: string;
  description: string;
  gate: string;
  triggerReason: string;
  boundFlightValue: string;
  submitting: boolean;
  canSubmit: boolean;
  businessCaseTypes: BusinessCaseTypeDefinition[];
  boundFlightBindingOptions: BoundFlightBindingOption[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'submit'): void;
  (e: 'update:eventType', value: string): void;
  (e: 'update:eventStatus', value: string): void;
  (e: 'update:description', value: string): void;
  (e: 'update:gate', value: string): void;
  (e: 'update:triggerReason', value: string): void;
  (e: 'update:boundFlightValue', value: string): void;
}>();
</script>

<template>
  <teleport to="body">
    <div
      v-if="isOpen"
      id="eventCreationModal"
      class="flight-monitor-event-modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="eventCreationTitle"
    >
      <div class="modal-content flight-monitor-event-modal-content">
        <div class="modal-header">
          <h2 id="eventCreationTitle">创建新业务事项</h2>
          <button
            type="button"
            class="close close-modal close-modal-compact"
            aria-label="关闭创建业务事项弹窗"
            @click="emit('close')"
          >
            &times;
          </button>
        </div>
        <form id="eventCreationForm" @submit.prevent="emit('submit')">
          <div class="event-form-grid">
            <div class="form-group">
              <label for="eventType">事项类型:</label>
              <select id="eventType" :value="eventType" required @change="emit('update:eventType', ($event.target as HTMLSelectElement).value)">
                <option value="">请选择事项类型</option>
                <option v-for="t in businessCaseTypes" :key="t.code" :value="t.code">{{ t.name }}</option>
              </select>
            </div>
            <div class="form-group">
              <label for="eventStatus">事项状态:</label>
              <select id="eventStatus" :value="eventStatus" required @change="emit('update:eventStatus', ($event.target as HTMLSelectElement).value)">
                <option value="INITIAL">初始</option>
                <option value="PENDING">待处理</option>
                <option value="PROCESSING">处理中</option>
                <option value="SUCCESS">成功</option>
                <option value="FAILED">失败</option>
              </select>
            </div>
            <div class="form-group">
              <label for="boundFlightNo">航班号绑定:</label>
              <select
                id="boundFlightNo"
                :value="boundFlightValue"
                :disabled="boundFlightBindingOptions.length === 0"
                required
                @change="emit('update:boundFlightValue', ($event.target as HTMLSelectElement).value)"
              >
                <option value="" disabled>
                  {{ boundFlightBindingOptions.length > 0 ? '请选择航班号' : '当前航班暂无可绑定航班号' }}
                </option>
                <option v-for="option in boundFlightBindingOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
              </select>
              <div
                v-if="boundFlightBindingOptions.length === 0"
                style="margin-top: 6px; font-size: 12px; color: var(--text-tertiary, #8e8e93);"
              >
                当前航班没有可用于绑定的进港/出港航班号。
              </div>
            </div>
            <div v-if="eventType === 'gate_baggage_check'" class="form-group">
              <label for="triggerReason">触发原因:</label>
              <input
                id="triggerReason"
                :value="triggerReason"
                type="text"
                required
                placeholder="需在此处说明触发原因"
                @input="emit('update:triggerReason', ($event.target as HTMLInputElement).value)"
              >
            </div>
            <div v-if="eventType === 'gate_baggage_check'" class="form-group">
              <label for="gate">登机口:</label>
              <input
                id="gate"
                :value="gate"
                type="text"
                required
                placeholder="请输入登机口"
                @input="emit('update:gate', ($event.target as HTMLInputElement).value)"
              >
            </div>
            <div class="form-group form-group-full">
              <label for="eventDescription">{{ eventType === 'gate_baggage_check' ? '额外信息补充:' : '事项描述:' }}</label>
              <textarea
                id="eventDescription"
                :value="description"
                :placeholder="eventType === 'gate_baggage_check' ? '请输入需要补充给通知对象的额外信息' : '请输入事项描述'"
                rows="3"
                style="width: 100%; border: 1px solid var(--border-light); border-radius: 4px; padding: 6px;"
                @input="emit('update:description', ($event.target as HTMLTextAreaElement).value)"
              />
            </div>
          </div>
          <div class="form-actions">
            <button type="submit" class="btn-save" :disabled="submitting || !canSubmit">
              {{ submitting ? '创建中...' : '创建事项' }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </teleport>
</template>

<style scoped>
/* 此前缺少遮罩定位样式：弹层会落到 body 底部且不可见，表现为「新建事项点了没反应」 */
.flight-monitor-event-modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 11000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  box-sizing: border-box;
  background: rgba(15, 23, 42, 0.48);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.flight-monitor-event-modal-content {
  width: min(560px, calc(100vw - 32px));
  max-height: min(86vh, 720px);
  overflow: auto;
  margin: 0;
  background: var(--bg-card, #fff);
  border: 1px solid var(--border-light, rgba(0, 0, 0, 0.08));
  border-radius: 14px;
  box-shadow: 0 20px 50px rgba(15, 23, 42, 0.28);
  padding: 20px 22px;
  color: var(--text-primary);
}

.flight-monitor-event-modal-content .modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-light);
}

.flight-monitor-event-modal-content .modal-header h2 {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
}

.flight-monitor-event-modal-content .form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 18px;
  padding-top: 12px;
  border-top: 1px solid var(--border-light);
}

[data-theme='dark'] .flight-monitor-event-modal-overlay {
  background: rgba(0, 0, 0, 0.62);
}
</style>
