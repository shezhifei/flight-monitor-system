<script setup lang="ts">
import UiModal from '../../../components/ui/UiModal.vue';
import UiButton from '../../../components/ui/UiButton.vue';
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
  <UiModal :open="isOpen" title="业务事项" :width="560" id="eventCreationModal" @close="emit('close')">
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
          <div v-if="boundFlightBindingOptions.length === 0" class="bind-hint">
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
            @input="emit('update:description', ($event.target as HTMLTextAreaElement).value)"
          />
        </div>
      </div>
    </form>
    <template #footer>
      <UiButton variant="primary" native-type="submit" form="eventCreationForm" :disabled="submitting || !canSubmit">
        {{ submitting ? '创建中...' : '创建事项' }}
      </UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.bind-hint {
  margin-top: 6px;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

#eventDescription {
  width: 100%;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-cell);
  padding: 6px 8px;
  background: var(--face-work);
  color: var(--ink);
  box-sizing: border-box;
}

#eventDescription:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
