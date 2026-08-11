<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type {
  CrewSlotRequirement,
  EquipmentRequirement,
  EquipmentTypeResponse,
  RequirementDraftPayload,
  RequirementPublishPayload,
  RequirementVersionResponse,
} from './dispatchRuleWorkbenchApi';

const props = defineProps<{
  taskTypeCode: string;
  versions: RequirementVersionResponse[];
  equipmentTypes: EquipmentTypeResponse[];
  disabled: boolean;
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'save-draft', payload: RequirementDraftPayload): void;
  (e: 'publish', payload: RequirementPublishPayload): void;
  (e: 'dirty', value: boolean): void;
}>();

const selectedVersionId = ref<string>('');
const draftNotes = ref('');
const crewRequirements = ref<CrewSlotRequirement[]>([]);
const equipmentRequirements = ref<EquipmentRequirement[]>([]);
const compareVersionId = ref<string>('');

const versionsForTask = computed(() =>
  props.versions.filter((v) => v.task_type === props.taskTypeCode),
);

const currentDraftVersion = computed(() =>
  versionsForTask.value.find((v) => v.status === 'draft') ?? null,
);

const publishedVersion = computed(() =>
  versionsForTask.value.find((v) => v.status === 'published') ?? null,
);

watch(
  () => [props.taskTypeCode, versionsForTask.value],
  () => {
    const base = currentDraftVersion.value ?? publishedVersion.value;
    if (base) {
      selectedVersionId.value = base.id;
      crewRequirements.value = base.crew_requirements.length
        ? deepClone(base.crew_requirements)
        : deepClone(base.requirements);
      equipmentRequirements.value = deepClone(base.equipment_requirements);
      draftNotes.value = base.notes ?? '';
    } else {
      selectedVersionId.value = '';
      crewRequirements.value = [];
      equipmentRequirements.value = [];
      draftNotes.value = '';
    }
    emit('dirty', false);
  },
  { immediate: true, deep: true },
);

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value));
}

function addCrew(): void {
  crewRequirements.value.push({
    slot_code: '',
    qualification_code: '',
    required_count: 1,
    must_be_distinct: true,
  });
  emit('dirty', true);
}

function removeCrew(idx: number): void {
  crewRequirements.value.splice(idx, 1);
  emit('dirty', true);
}

function addEquipment(): void {
  equipmentRequirements.value.push({
    slot_code: '',
    equipment_type_id: null,
    required_count: 1,
    must_be_distinct: true,
    requires_driver: false,
  });
  emit('dirty', true);
}

function removeEquipment(idx: number): void {
  equipmentRequirements.value.splice(idx, 1);
  emit('dirty', true);
}

function onFieldChange(): void {
  emit('dirty', true);
}

const compareVersion = computed(() =>
  versionsForTask.value.find((v) => v.id === compareVersionId.value) ?? null,
);

const compareSummary = computed(() => {
  if (!compareVersion.value) return null;
  return {
    crew: compareVersion.value.crew_requirements.length || compareVersion.value.requirements.length,
    equipment: compareVersion.value.equipment_requirements.length,
    status: compareVersion.value.status,
    version_no: compareVersion.value.version_no,
  };
});

function saveDraft(): void {
  if (!props.taskTypeCode) return;
  const payload: RequirementDraftPayload = {
    task_type: props.taskTypeCode,
    crew_requirements: crewRequirements.value,
    requirements: crewRequirements.value,
    equipment_requirements: equipmentRequirements.value,
    notes: draftNotes.value || null,
  };
  emit('save-draft', payload);
}

function publishVersion(): void {
  if (!props.taskTypeCode) return;
  const draftId = currentDraftVersion.value?.id ?? null;
  if (!draftId) {
    if (!window.confirm('当前没有未发布的草稿，是否仍要发布最新版本？')) return;
  }
  const payload: RequirementPublishPayload = {
    task_type: props.taskTypeCode,
    draft_id: draftId,
  };
  emit('publish', payload);
}
</script>

<template>
  <section class="requirement-panel" aria-label="资质要求">
    <header class="head">
      <div>
        <h3>资质与设备需求</h3>
        <p class="muted">
          已发布: <span v-if="publishedVersion">v{{ publishedVersion.version_no }}</span><span v-else>无</span>
          · 草稿: <span v-if="currentDraftVersion">v{{ currentDraftVersion.version_no }}</span><span v-else>无</span>
        </p>
      </div>
      <div class="head-actions">
        <button
          type="button"
          class="btn"
          :disabled="disabled || saving"
          @click="saveDraft"
        >
          {{ saving ? '保存中…' : '保存草稿' }}
        </button>
        <button
          type="button"
          class="btn primary"
          :disabled="disabled || saving"
          @click="publishVersion"
        >
          发布为新版本
        </button>
      </div>
    </header>

    <div class="section">
      <div class="section-head">
        <h4>机组岗位需求</h4>
        <button
          type="button"
          class="ghost-btn"
          :disabled="disabled"
          @click="addCrew"
        >
          + 新增岗位
        </button>
      </div>
      <table v-if="crewRequirements.length" class="req-table">
        <thead>
          <tr>
            <th>岗位编码</th>
            <th>资质编码</th>
            <th>最低等级</th>
            <th>人数</th>
            <th>独占组</th>
            <th>备注</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, idx) in crewRequirements" :key="idx">
            <td>
              <input
                v-model="row.slot_code"
                type="text"
                required
                @input="onFieldChange"
              >
            </td>
            <td>
              <input
                v-model="row.qualification_code"
                type="text"
                required
                @input="onFieldChange"
              >
            </td>
            <td><input v-model="row.min_level_code" type="text" @input="onFieldChange"></td>
            <td>
              <input
                v-model.number="row.required_count"
                type="number"
                min="1"
                @input="onFieldChange"
              >
            </td>
            <td><input v-model="row.exclusive_group" type="text" @input="onFieldChange"></td>
            <td><input v-model="row.remarks" type="text" @input="onFieldChange"></td>
            <td>
              <button
                type="button"
                class="ghost-btn danger"
                :disabled="disabled"
                @click="removeCrew(idx)"
              >
                移除
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">
        尚未配置机组岗位
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <h4>设备需求</h4>
        <button
          type="button"
          class="ghost-btn"
          :disabled="disabled"
          @click="addEquipment"
        >
          + 新增设备
        </button>
      </div>
      <table v-if="equipmentRequirements.length" class="req-table">
        <thead>
          <tr>
            <th>设备槽位</th>
            <th>设备类型</th>
            <th>数量</th>
            <th>需要驾驶员</th>
            <th>驾驶员资质</th>
            <th>备注</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, idx) in equipmentRequirements" :key="idx">
            <td>
              <input
                v-model="row.slot_code"
                type="text"
                required
                @input="onFieldChange"
              >
            </td>
            <td>
              <select v-model="row.equipment_type_id" @change="onFieldChange">
                <option :value="null">
                  — 任意 —
                </option>
                <option v-for="eq in equipmentTypes" :key="eq.id" :value="eq.id">
                  {{ eq.name }} ({{ eq.code || eq.id }})
                </option>
              </select>
            </td>
            <td>
              <input
                v-model.number="row.required_count"
                type="number"
                min="1"
                @input="onFieldChange"
              >
            </td>
            <td>
              <label class="checkbox">
                <input v-model="row.requires_driver" type="checkbox" @change="onFieldChange"> 是
              </label>
            </td>
            <td><input v-model="row.driver_qualification_code" type="text" @input="onFieldChange"></td>
            <td><input v-model="row.remarks" type="text" @input="onFieldChange"></td>
            <td>
              <button
                type="button"
                class="ghost-btn danger"
                :disabled="disabled"
                @click="removeEquipment(idx)"
              >
                移除
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">
        尚未配置设备需求
      </div>
    </div>

    <div class="section">
      <div class="section-head">
        <h4>说明</h4>
      </div>
      <textarea
        v-model="draftNotes"
        rows="2"
        class="notes"
        placeholder="变更说明"
        @input="onFieldChange"
      />
    </div>

    <div class="section">
      <div class="section-head">
        <h4>版本对比</h4>
        <select v-model="compareVersionId">
          <option value="">
            — 选择历史版本 —
          </option>
          <option v-for="v in versionsForTask" :key="v.id" :value="v.id">
            v{{ v.version_no }} · {{ v.status }}
          </option>
        </select>
      </div>
      <div v-if="compareSummary" class="compare">
        <div>状态: <strong>{{ compareSummary.status }}</strong></div>
        <div>版本号: <strong>v{{ compareSummary.version_no }}</strong></div>
        <div>岗位数: <strong>{{ compareSummary.crew }}</strong> / 当前草稿: <strong>{{ crewRequirements.length }}</strong></div>
        <div>设备数: <strong>{{ compareSummary.equipment }}</strong> / 当前草稿: <strong>{{ equipmentRequirements.length }}</strong></div>
      </div>
      <div v-else class="empty">
        选择一个版本以对比当前草稿
      </div>
    </div>
  </section>
</template>

<style scoped>
.requirement-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}
.head h3 { margin: 0; font-size: 16px; }
.muted { font-size: 12px; color: var(--text-tertiary); margin: 4px 0 0; }
.head-actions { display: flex; gap: 8px; }
.btn {
  border: 1px solid var(--border-light);
  border-radius: 8px;
  background: var(--bg-card);
  padding: 6px 14px;
  cursor: pointer;
  font-size: 13px;
}
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn:disabled, .ghost-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.ghost-btn {
  border: none;
  background: transparent;
  color: var(--system-blue);
  cursor: pointer;
  font-size: 12px;
}
.ghost-btn.danger { color: var(--system-red); }
.section { border: 1px solid var(--border-light); border-radius: 8px; padding: 12px; }
.section-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}
.section-head h4 { margin: 0; font-size: 13px; }
.req-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.req-table th, .req-table td {
  border: 1px solid #f1f1f1;
  padding: 4px 6px;
}
.req-table input, .req-table select {
  width: 100%;
  border: 1px solid transparent;
  background: transparent;
  font-size: 12px;
}
.req-table input:focus, .req-table select:focus {
  border-color: var(--system-blue);
  outline: none;
}
.checkbox { display: inline-flex; gap: 4px; align-items: center; font-size: 12px; }
.empty { padding: 16px; text-align: center; color: var(--text-tertiary); font-size: 12px; }
.notes {
  width: 100%;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  padding: 6px;
  font-size: 12px;
}
.compare {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 6px;
  font-size: 12px;
}
</style>
