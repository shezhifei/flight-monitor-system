<script setup lang="ts">
import { computed, ref } from 'vue';
import type { DispatchRulePreviewResponse, TaskTypeResponse } from './dispatchRuleWorkbenchApi';

const props = withDefaults(defineProps<{
  taskTypes: TaskTypeResponse[];
  result: DispatchRulePreviewResponse | null;
  saving: boolean;
  disabled?: boolean;
}>(), {
  disabled: false,
});

const emit = defineEmits<{
  (e: 'preview', payload: { flight_id?: string | null; sample_flight: Record<string, unknown> }): void;
}>();

interface PreviewForm {
  flight_id: string;
  task_type: string;
  leg_scope: string;
  flight_nature: string;
  flight_status: string;
  terminal: string;
  aircraft_type: string;
  stand_type: string;
  vip: boolean;
  turnaround: boolean;
  boarding_restriction: boolean;
  quick_turnaround: boolean;
  commercial_signed: boolean;
  delta_t_minutes: number | null;
  minimum_turnaround_minutes: number | null;
}

const form = ref<PreviewForm>({
  flight_id: '',
  task_type: '',
  leg_scope: 'outbound',
  flight_nature: 'domestic',
  flight_status: 'scheduled',
  terminal: '',
  aircraft_type: '',
  stand_type: '',
  vip: false,
  turnaround: false,
  boarding_restriction: false,
  quick_turnaround: false,
  commercial_signed: false,
  delta_t_minutes: null,
  minimum_turnaround_minutes: null,
});
const formError = ref('');
const flightStatusOptions = [
  { value: 'scheduled', label: '计划' },
  { value: 'boarding', label: '登机' },
  { value: 'delayed', label: '延误' },
  { value: 'departed', label: '已起飞' },
  { value: 'arrived', label: '已到达' },
  { value: 'canceled', label: '取消' },
];

function buildPayload() {
  const sample: Record<string, unknown> = {
    task_type: form.value.task_type,
    leg_scope: form.value.leg_scope,
    flight_nature: form.value.flight_nature || null,
    flight_status: form.value.flight_status || null,
    terminal: form.value.terminal || null,
    aircraft_type: form.value.aircraft_type || null,
    stand_type: form.value.stand_type || null,
    vip: form.value.vip,
    turnaround: form.value.turnaround,
    boarding_restriction: form.value.boarding_restriction,
    quick_turnaround: form.value.quick_turnaround,
    commercial_signed: form.value.commercial_signed,
    delta_t_minutes: form.value.delta_t_minutes,
    minimum_turnaround_minutes: form.value.minimum_turnaround_minutes,
  };
  return {
    flight_id: form.value.flight_id || null,
    sample_flight: sample,
  };
}

function runPreview(): void {
  formError.value = '';
  if (!form.value.task_type.trim()) {
    formError.value = '请选择任务类型后再运行预览。';
    return;
  }
  emit('preview', buildPayload());
}

const generatedOrders = computed(() => props.result?.generated_orders ?? []);
const adjustments = computed(() => props.result?.applied_adjustments ?? []);
const conflicts = computed(() => props.result?.conflicts ?? []);
const turnaroundConstraints = computed(() => props.result?.turnaround_constraints ?? []);
const blockingErrors = computed(() => props.result?.blocking_errors ?? []);

function stringify(value: unknown): string {
  return JSON.stringify(value, null, 2);
}
</script>

<template>
  <section class="rule-preview-panel" aria-label="规则预览">
    <header class="head">
      <h3>规则预览</h3>
      <p class="muted">
        调用 <code>POST /api/v2/dispatch/rules/preview</code> 模拟单航班
      </p>
    </header>

    <form class="preview-form" @submit.prevent="runPreview">
      <div class="form-grid">
        <label>航班 ID
          <input v-model="form.flight_id" type="text" placeholder="可选，留空使用 sample_flight">
        </label>
        <label>任务类型
          <select v-model="form.task_type">
            <option value="">请选择任务类型</option>
            <option v-for="t in taskTypes" :key="t.code" :value="t.code">{{ t.name }} ({{ t.code }})</option>
          </select>
        </label>
        <label>航段
          <select v-model="form.leg_scope">
            <option value="outbound">出港</option>
            <option value="inbound">进港</option>
            <option value="both">双向</option>
          </select>
        </label>
        <label>航班性质
          <select v-model="form.flight_nature">
            <option value="domestic">国内</option>
            <option value="international">国际</option>
            <option value="regional">地区</option>
          </select>
        </label>
        <label>航班状态
          <select v-model="form.flight_status">
            <option v-for="status in flightStatusOptions" :key="status.value" :value="status.value">
              {{ status.label }} ({{ status.value }})
            </option>
          </select>
        </label>
        <label>航站楼
          <input v-model="form.terminal" type="text">
        </label>
        <label>机型
          <input v-model="form.aircraft_type" type="text">
        </label>
        <label>机位类型
          <input v-model="form.stand_type" type="text">
        </label>
        <label>Δ-T (分)
          <input v-model.number="form.delta_t_minutes" type="number">
        </label>
        <label>最小过站时长 (分)
          <input v-model.number="form.minimum_turnaround_minutes" type="number">
        </label>
      </div>

      <div class="flag-row">
        <label class="checkbox"><input v-model="form.vip" type="checkbox"> VIP</label>
        <label class="checkbox"><input v-model="form.turnaround" type="checkbox"> 过站</label>
        <label class="checkbox"><input v-model="form.boarding_restriction" type="checkbox"> 限制登机</label>
        <label class="checkbox"><input v-model="form.quick_turnaround" type="checkbox"> 快速过站</label>
        <label class="checkbox"><input v-model="form.commercial_signed" type="checkbox"> 商载签字</label>
      </div>

      <div class="form-actions">
        <span v-if="formError" class="form-error" role="alert">{{ formError }}</span>
        <button type="submit" class="btn primary" :disabled="disabled || saving">
          {{ saving ? '预览中…' : '运行预览' }}
        </button>
      </div>
    </form>

    <div v-if="result" class="result-grid">
      <article>
        <h4>生成派工 ({{ generatedOrders.length }})</h4>
        <pre v-if="generatedOrders.length">{{ stringify(generatedOrders) }}</pre>
        <div v-else class="empty">
          无生成派工
        </div>
      </article>
      <article>
        <h4>应用调整 ({{ adjustments.length }})</h4>
        <pre v-if="adjustments.length">{{ stringify(adjustments) }}</pre>
        <div v-else class="empty">
          无调整动作
        </div>
      </article>
      <article>
        <h4>过站约束 ({{ turnaroundConstraints.length }})</h4>
        <pre v-if="turnaroundConstraints.length">{{ stringify(turnaroundConstraints) }}</pre>
        <div v-else class="empty">
          无过站约束
        </div>
      </article>
      <article>
        <h4>冲突 ({{ conflicts.length }})</h4>
        <pre v-if="conflicts.length">{{ stringify(conflicts) }}</pre>
        <div v-else class="empty">
          无冲突
        </div>
      </article>
      <article v-if="blockingErrors.length" class="error-card">
        <h4>阻断性错误</h4>
        <ul>
          <li v-for="(err, idx) in blockingErrors" :key="idx">
            {{ err }}
          </li>
        </ul>
      </article>
    </div>
    <div v-else class="empty hint">
      请填写参数并运行预览
    </div>
  </section>
</template>

<style scoped>
.rule-preview-panel { display: flex; flex-direction: column; gap: 12px; }
.head h3 { margin: 0; font-size: 16px; }
.head .muted { font-size: 12px; color: var(--text-tertiary); margin: 4px 0 0; }
.head code { background: var(--ws-surface-muted); padding: 1px 4px; border-radius: 4px; }
.preview-form {
  border: 1px solid var(--border-light);
  border-radius: 10px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.form-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px;
}
.form-grid label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-secondary); }
.form-grid input, .form-grid select {
  padding: 6px 8px; border: 1px solid var(--border-light); border-radius: 6px; font-size: 12px;
}
.flag-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  border-top: 1px dashed var(--border-light);
  padding-top: 8px;
}
.checkbox { display: inline-flex; gap: 6px; align-items: center; font-size: 12px; }
.form-actions { display: flex; justify-content: flex-end; }
.form-error { margin-right: auto; color: var(--ws-danger); font-size: 12px; align-self: center; }
.btn { border: 1px solid var(--border-light); border-radius: 8px; padding: 6px 14px; background: var(--bg-card); cursor: pointer; font-size: 13px; }
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.result-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}
.result-grid article {
  border: 1px solid var(--border-light);
  border-radius: 8px;
  padding: 10px;
}
.result-grid h4 { margin: 0 0 8px; font-size: 13px; }
.result-grid pre {
  background: #0f172a; color: #f1f5f9; padding: 10px; border-radius: 6px;
  font-size: 11px; max-height: 220px; overflow: auto; margin: 0;
}
.empty { padding: 12px; text-align: center; font-size: 12px; color: var(--text-tertiary); }
.empty.hint {
  border: 1px dashed var(--border-light);
  border-radius: 10px;
}
.error-card { grid-column: span 2; background: var(--error-bg-subtle); border-color: var(--error-border-subtle); }
.error-card ul { margin: 0; padding-left: 20px; font-size: 12px; color: var(--ws-danger); }
</style>
