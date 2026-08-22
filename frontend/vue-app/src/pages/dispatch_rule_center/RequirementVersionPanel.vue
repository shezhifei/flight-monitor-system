<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
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

/* 行内下拉：数组行逐行回写，不走 computed 桥 */
const equipmentTypeOptions = computed(() => [
  { value: '', label: '— 任意 —' },
  ...props.equipmentTypes.map((eq) => ({
    value: eq.id,
    label: `${eq.name} (${eq.code || eq.id})`,
  })),
]);

function setEquipmentType(row: EquipmentRequirement, value: string): void {
  row.equipment_type_id = value || null;
  onFieldChange();
}

const compareOptions = computed(() => [
  { value: '', label: '— 选择历史版本 —' },
  ...versionsForTask.value.map((v) => ({
    value: v.id,
    label: `v${v.version_no} · ${v.status}`,
  })),
]);

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
        <UiButton
          :disabled="disabled || saving"
          @click="saveDraft"
        >
          {{ saving ? '保存中…' : '保存草稿' }}
        </UiButton>
        <UiButton
          variant="primary"
          :disabled="disabled || saving"
          @click="publishVersion"
        >
          发布为新版本
        </UiButton>
      </div>
    </header>

    <div class="section">
      <div class="section-head">
        <h4>机组岗位需求</h4>
        <UiButton
          variant="quiet"
          :disabled="disabled"
          @click="addCrew"
        >
          + 新增岗位
        </UiButton>
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
              <UiButton
                variant="danger"
                :disabled="disabled"
                @click="removeCrew(idx)"
              >
                移除
              </UiButton>
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
        <UiButton
          variant="quiet"
          :disabled="disabled"
          @click="addEquipment"
        >
          + 新增设备
        </UiButton>
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
              <UiSelect
                :model-value="row.equipment_type_id ?? ''"
                :options="equipmentTypeOptions"
                label="设备类型"
                min-width="100%"
                @update:model-value="setEquipmentType(row, $event)"
              />
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
              <UiButton
                variant="danger"
                :disabled="disabled"
                @click="removeEquipment(idx)"
              >
                移除
              </UiButton>
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
        <UiSelect
          v-model="compareVersionId"
          :options="compareOptions"
          label="版本对比"
        />
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
/* 按钮归 UiButton、下拉归 UiSelect；这里只留需求表格的行内编辑 */
.requirement-panel {
  display: flex;
  flex-direction: column;
  gap: var(--s4);
}

.head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}

.head h3 {
  margin: 0;
  font-size: var(--fs-title);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.muted {
  font-size: var(--fs-label);
  color: var(--ink-muted);
  margin: var(--s1) 0 0;
}

.head-actions {
  display: flex;
  gap: var(--s2);
}

.section {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  padding: var(--s3);
}

.section-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--s2);
}

.section-head h4 {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.req-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-label);
}

.req-table th,
.req-table td {
  border: 1px solid var(--line);
  padding: var(--s1) var(--s2);
}

.req-table input {
  width: 100%;
  border: 1px solid transparent;
  background: transparent;
  font-size: var(--fs-label);
  color: var(--ink);
  font-family: inherit;
}

/* 行内编辑：聚焦才描边，不抢表格视线 */
.req-table input:focus {
  border-color: var(--act);
  outline: none;
}

.checkbox {
  display: inline-flex;
  gap: var(--s1);
  align-items: center;
  font-size: var(--fs-label);
}

.checkbox input {
  accent-color: var(--act);
}

.empty {
  padding: var(--s4);
  text-align: center;
  color: var(--ink-muted);
  font-size: var(--fs-label);
}

.notes {
  width: 100%;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-cell);
  padding: var(--s2);
  font-size: var(--fs-body);
  color: var(--ink);
  background: var(--face-page);
  font-family: inherit;
  box-sizing: border-box;
}

.notes:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.compare {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--s2);
  font-size: var(--fs-label);
}
</style>
