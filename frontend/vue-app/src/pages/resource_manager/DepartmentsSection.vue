<script setup lang="ts">
import type { Department, DepartmentFormData } from '@/composables/useResourceManager';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSearch from '@/components/ui/UiSearch.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const props = defineProps<{
  active: boolean;
  canManage: boolean;
  departments: Department[];
  total: number;
  search: string;
  saving: boolean;
  modalOpen: boolean;
  editing: Department | null;
  form: DepartmentFormData;
  managerOptions: Array<{ value: string; label: string }>;
}>();

const emit = defineEmits<{
  (e: 'update:search', value: string): void;
  (e: 'update:form', value: DepartmentFormData): void;
  (e: 'open', item?: Department): void;
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'toggle-active', item: Department): void;
}>();

function patch<K extends keyof DepartmentFormData>(field: K, value: DepartmentFormData[K]) {
  emit('update:form', { ...props.form, [field]: value });
}
</script>

<template>
  <section class="section-content" :class="{ active }">
    <div class="content-header">
      <div class="content-heading">
        <div class="content-title">
          科室目录
        </div>
        <div class="content-subtitle">
          班组与设备直接挂科室；科室负责人用于审批与通知。
        </div>
      </div>
    </div>
    <div class="content-body">
      <div class="section-toolbar">
        <div class="filter-group">
          <UiSearch
            :model-value="search"
            label="搜索科室"
            placeholder="搜索科室..."
            @update:model-value="emit('update:search', $event)"
          />
        </div>
        <UiButton
          v-if="canManage"
          variant="primary"
          size="md"
          @click="emit('open')"
        >
          <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建科室
        </UiButton>
      </div>

      <div class="table-container">
        <table>
          <thead>
            <tr>
              <th>名称</th>
              <th>代码</th>
              <th>负责人</th>
              <th>描述</th>
              <th>状态</th>
              <th class="col-actions">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="departments.length === 0">
              <td colspan="6" class="empty-state">
                暂无科室
              </td>
            </tr>
            <tr v-for="dept in departments" :key="dept.id">
              <td><strong>{{ dept.name }}</strong></td>
              <td>{{ dept.code || '-' }}</td>
              <td>{{ dept.manager_name || '-' }}</td>
              <td>{{ dept.description || '-' }}</td>
              <td>
                <UiPill :tone="dept.is_active === false ? 'mute' : 'ok'">
                  {{ dept.is_active === false ? '已停用' : '启用中' }}
                </UiPill>
              </td>
              <td>
                <div class="row-actions">
                  <UiButton v-if="canManage" @click="emit('open', dept)">
                    编辑
                  </UiButton>
                  <UiButton
                    v-if="canManage"
                    :variant="dept.is_active === false ? 'tonal' : 'danger'"
                    @click="emit('toggle-active', dept)"
                  >
                    {{ dept.is_active === false ? '启用' : '停用' }}
                  </UiButton>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="pagination">
        <span class="pagination-info">共 {{ total }} 条</span>
      </div>
    </div>

    <UiModal
      :open="modalOpen"
      :title="editing ? '编辑科室' : '新建科室'"
      :width="480"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="d-name">名称 <span class="required">*</span></label>
        <input
          id="d-name"
          type="text"
          :value="form.name"
          placeholder="例如：地面服务部"
          @input="patch('name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="d-code">代码</label>
        <input
          id="d-code"
          type="text"
          :value="form.code"
          placeholder="例如：GROUND"
          @input="patch('code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <UiSelect
          :model-value="form.manager_id"
          :options="managerOptions"
          label="负责人"
          min-width="100%"
          @update:model-value="patch('manager_id', $event)"
        />
      </div>
      <div class="form-group">
        <label for="d-desc">描述</label>
        <textarea
          id="d-desc"
          :value="form.description"
          placeholder="可选"
          @input="patch('description', ($event.target as HTMLTextAreaElement).value)"
        />
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!form.name.trim() || saving"
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

.form-group input[type="text"],
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

.form-group input[type="text"] {
  padding: 0 var(--s3);
  height: var(--h-md);
}

.form-group textarea {
  min-height: 72px;
  resize: vertical;
}

.form-group input:focus-visible,
.form-group textarea:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}
</style>
