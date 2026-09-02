<script setup lang="ts">
import { computed } from 'vue';
import type {
  QualificationCatalog,
  QualificationFormData,
  QualificationLevel,
  QualificationLevelFormData,
  QualificationModal,
} from '@/composables/useQualificationCatalog';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSearch from '@/components/ui/UiSearch.vue';
import UiSelect from '@/components/ui/UiSelect.vue';
import FieldOverlayForm from '@/components/FieldOverlayForm.vue';
import type { FieldOverlay, FieldReferenceEntry } from '@/composables/useFieldOverlays';

const props = defineProps<{
  active: boolean;
  canManage: boolean;
  selectedDepartmentId: string;
  catalogs: QualificationCatalog[];
  search: string;
  loading: boolean;
  saving: boolean;
  modal: QualificationModal;
  form: QualificationFormData;
  levelForm: QualificationLevelFormData;
  departmentOptions: Array<{ value: string; label: string }>;
  fieldOverlays?: FieldOverlay[];
  fieldCatalogEntries?: Record<string, Array<{ code: string; name: string }>>;
  fieldReferenceEntries?: Record<string, FieldReferenceEntry[]>;
  levelsFor: (code: string) => QualificationLevel[];
}>();

const emit = defineEmits<{
  (e: 'update:selectedDepartmentId', value: string): void;
  (e: 'update:search', value: string): void;
  (e: 'update:form', value: QualificationFormData): void;
  (e: 'update:levelForm', value: QualificationLevelFormData): void;
  (e: 'open', item?: QualificationCatalog): void;
  (e: 'open-level', item: QualificationCatalog): void;
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'toggle-active', item: QualificationCatalog): void;
}>();

const qualificationModalShow = computed(() => props.modal.kind === 'qualification');
const levelModalShow = computed(() => props.modal.kind === 'level');
const editing = computed(() => (props.modal.kind === 'qualification' ? props.modal.item ?? null : null));
const levelTarget = computed(() => (props.modal.kind === 'level' ? props.modal.qualification : null));

function patchForm<K extends keyof QualificationFormData>(field: K, value: QualificationFormData[K]) {
  emit('update:form', { ...props.form, [field]: value });
}
function patchLevel<K extends keyof QualificationLevelFormData>(field: K, value: QualificationLevelFormData[K]) {
  emit('update:levelForm', { ...props.levelForm, [field]: value });
}

function levelSummary(code: string): string {
  const items = props.levelsFor(code).filter((l) => l.is_active);
  if (items.length === 0) return '—';
  return items.map((l) => `${l.level_name}(${l.level_code})`).join('、');
}
</script>

<template>
  <section class="section-content" :class="{ active }">
    <div class="content-header">
      <div class="content-heading">
        <div class="content-title">
          资质目录
        </div>
        <div class="content-subtitle">
          科室自己的资质编码、名称与等级。发放到个人在用户管理页。
        </div>
      </div>
    </div>
    <div class="content-body">
      <div class="section-toolbar">
        <div class="filter-group">
          <UiSelect
            :model-value="selectedDepartmentId"
            :options="departmentOptions"
            label="所属科室"
            min-width="220px"
            @update:model-value="emit('update:selectedDepartmentId', $event)"
          />
          <UiSearch
            :model-value="search"
            label="搜索资质"
            placeholder="搜索编码或名称..."
            @update:model-value="emit('update:search', $event)"
          />
        </div>
        <UiButton
          v-if="canManage"
          variant="primary"
          size="md"
          :disabled="!selectedDepartmentId"
          @click="emit('open')"
        >
          <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建资质
        </UiButton>
      </div>

      <div v-if="!selectedDepartmentId" class="empty-state">
        请先选择科室。
      </div>
      <div v-else-if="loading" class="empty-state">
        加载资质目录...
      </div>
      <div v-else class="table-container">
        <table>
          <thead>
            <tr>
              <th>编码</th>
              <th>名称</th>
              <th>等级</th>
              <th>状态</th>
              <th class="col-actions">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="catalogs.length === 0">
              <td colspan="5" class="empty-state">
                本科室暂无资质目录
              </td>
            </tr>
            <tr v-for="item in catalogs" :key="item.id">
              <td><strong>{{ item.qualification_code }}</strong></td>
              <td>{{ item.qualification_name }}</td>
              <td>{{ levelSummary(item.qualification_code) }}</td>
              <td>
                <UiPill :tone="item.is_active ? 'ok' : 'mute'">
                  {{ item.is_active ? '启用中' : '已停用' }}
                </UiPill>
              </td>
              <td>
                <div class="row-actions">
                  <UiButton v-if="canManage" @click="emit('open', item)">
                    编辑
                  </UiButton>
                  <UiButton v-if="canManage" variant="tonal" @click="emit('open-level', item)">
                    加等级
                  </UiButton>
                  <UiButton
                    v-if="canManage"
                    :variant="item.is_active ? 'danger' : 'tonal'"
                    @click="emit('toggle-active', item)"
                  >
                    {{ item.is_active ? '停用' : '启用' }}
                  </UiButton>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <UiModal
      :open="qualificationModalShow"
      :title="editing ? '编辑资质' : '新建资质'"
      :width="480"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="q-code">编码 <span class="required">*</span></label>
        <input
          id="q-code"
          type="text"
          :value="form.qualification_code"
          placeholder="例如：TOWING"
          :disabled="Boolean(editing)"
          @input="patchForm('qualification_code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <FieldOverlayForm
        :model-value="form.attributes ?? {}"
        :overlays="fieldOverlays ?? []"
        :catalog-entries="fieldCatalogEntries ?? {}"
        :reference-entries="fieldReferenceEntries ?? {}"
        @update:model-value="patchForm('attributes', $event)"
      />
      <div class="form-group">
        <label for="q-name">名称 <span class="required">*</span></label>
        <input
          id="q-name"
          type="text"
          :value="form.qualification_name"
          placeholder="例如：牵引"
          @input="patchForm('qualification_name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="q-desc">描述</label>
        <textarea
          id="q-desc"
          :value="form.description"
          placeholder="可选"
          @input="patchForm('description', ($event.target as HTMLTextAreaElement).value)"
        />
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!form.qualification_code.trim() || !form.qualification_name.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <UiModal
      :open="levelModalShow"
      :title="`为 ${levelTarget?.qualification_name ?? ''} 添加等级`"
      :width="480"
      @close="emit('close')"
    >
      <p v-if="levelTarget" class="form-hint">
        已有：{{ levelSummary(levelTarget.qualification_code) }}
      </p>
      <div class="form-group">
        <label for="l-code">等级编码 <span class="required">*</span></label>
        <input
          id="l-code"
          type="text"
          :value="levelForm.level_code"
          placeholder="例如：senior"
          @input="patchLevel('level_code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="l-name">等级名称 <span class="required">*</span></label>
        <input
          id="l-name"
          type="text"
          :value="levelForm.level_name"
          placeholder="例如：高级"
          @input="patchLevel('level_name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="l-rank">排序（数字越大越高）</label>
        <input
          id="l-rank"
          type="number"
          :value="levelForm.level_rank"
          min="1"
          @input="patchLevel('level_rank', Number(($event.target as HTMLInputElement).value) || 1)"
        >
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!levelForm.level_code.trim() || !levelForm.level_name.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>
  </section>
</template>

<style scoped>
.section-content {
  display: none;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.section-content.active {
  display: flex;
}

.section-content .content-header {
  flex-shrink: 0;
}

.section-content .content-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.row-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--s1);
}

.col-actions {
  text-align: right;
}

.filter-group {
  display: flex;
  gap: var(--s2);
  align-items: end;
}

.form-group {
  margin-bottom: var(--s3);
}

.form-group > label {
  display: block;
  font-size: var(--fs-label);
  font-weight: var(--fw-medium);
  margin-bottom: var(--s1);
  color: var(--ink-subtle);
}

.required {
  color: var(--danger);
}

.form-hint {
  margin: 0 0 var(--s3);
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.form-group input[type="text"],
.form-group input[type="number"],
.form-group textarea {
  width: 100%;
  min-height: var(--h-md);
  padding: var(--s1) var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  background: var(--face-page);
  color: var(--ink);
  box-sizing: border-box;
  font-family: inherit;
}

.form-group input[type="text"],
.form-group input[type="number"] {
  padding: 0 var(--s3);
  height: var(--h-md);
}

.form-group textarea {
  min-height: 72px;
  resize: vertical;
}
</style>
