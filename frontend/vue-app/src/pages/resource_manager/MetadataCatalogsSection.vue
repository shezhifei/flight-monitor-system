<script setup lang="ts">
import { computed } from 'vue';
import type {
  CatalogFormData,
  EntryFormData,
  MetadataCatalog,
  MetadataCatalogEntry,
  MetadataCatalogModal,
} from '@/composables/useMetadataCatalog';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSearch from '@/components/ui/UiSearch.vue';

const props = defineProps<{
  active: boolean;
  canManage: boolean;
  catalogs: MetadataCatalog[];
  selectedCode: string;
  entries: MetadataCatalogEntry[];
  search: string;
  loading: boolean;
  saving: boolean;
  modal: MetadataCatalogModal;
  catalogForm: CatalogFormData;
  entryForm: EntryFormData;
}>();

const emit = defineEmits<{
  (e: 'update:selectedCode', value: string): void;
  (e: 'update:search', value: string): void;
  (e: 'update:catalogForm', value: CatalogFormData): void;
  (e: 'update:entryForm', value: EntryFormData): void;
  (e: 'open-catalog', item?: MetadataCatalog): void;
  (e: 'open-entry', item?: MetadataCatalogEntry): void;
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'toggle-catalog', item: MetadataCatalog): void;
  (e: 'toggle-entry', item: MetadataCatalogEntry): void;
}>();

const catalogModalShow = computed(() => props.modal.kind === 'catalog');
const entryModalShow = computed(() => props.modal.kind === 'entry');
const editingCatalog = computed(() => (props.modal.kind === 'catalog' ? props.modal.item ?? null : null));
const editingEntry = computed(() => (props.modal.kind === 'entry' ? props.modal.item ?? null : null));
const selectedCatalog = computed(() => props.catalogs.find((c) => c.code === props.selectedCode) ?? null);

function patchCatalog<K extends keyof CatalogFormData>(field: K, value: CatalogFormData[K]) {
  emit('update:catalogForm', { ...props.catalogForm, [field]: value });
}
function patchEntry<K extends keyof EntryFormData>(field: K, value: EntryFormData[K]) {
  emit('update:entryForm', { ...props.entryForm, [field]: value });
}

function sourceLabel(source: string) {
  return source === 'ingest' ? '导入' : '手工';
}
</script>

<template>
  <section class="section-content" :class="{ active }">
    <div class="content-header">
      <div class="content-heading">
        <div class="content-title">
          码表
        </div>
        <div class="content-subtitle">
          机型、ICAO 等级等取值目录。机型开放：导入未见过的字符串会自动加一行。
        </div>
      </div>
    </div>
    <div class="content-body">
      <div class="section-toolbar">
        <div class="filter-group">
          <UiSearch
            :model-value="search"
            label="搜索码表项"
            placeholder="搜索编码或名称..."
            @update:model-value="emit('update:search', $event)"
          />
        </div>
        <div class="row-actions">
          <UiButton v-if="canManage" size="md" @click="emit('open-catalog')">
            <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建码表
          </UiButton>
          <UiButton
            v-if="canManage"
            variant="primary"
            size="md"
            :disabled="!selectedCode"
            @click="emit('open-entry')"
          >
            <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建项
          </UiButton>
        </div>
      </div>

      <div v-if="loading" class="empty-state">
        加载码表...
      </div>
      <div v-else class="table-container">
        <table>
          <thead>
            <tr>
              <th>码表</th>
              <th>类型</th>
              <th>状态</th>
              <th class="col-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="catalogs.length === 0">
              <td colspan="4" class="empty-state">暂无码表</td>
            </tr>
            <tr
              v-for="item in catalogs"
              :key="item.code"
              :class="{ selected: item.code === selectedCode }"
              @click="emit('update:selectedCode', item.code)"
            >
              <td>
                <strong>{{ item.name }}</strong>
                <div class="muted">{{ item.code }}</div>
              </td>
              <td>{{ item.is_open ? '开放' : '封闭' }}{{ item.is_ordered ? ' · 有序' : '' }}</td>
              <td>
                <UiPill :tone="item.is_active ? 'ok' : 'mute'">
                  {{ item.is_active ? '启用中' : '已停用' }}
                </UiPill>
              </td>
              <td>
                <div class="row-actions">
                  <UiButton v-if="canManage" @click.stop="emit('open-catalog', item)">编辑</UiButton>
                  <UiButton
                    v-if="canManage && !item.system_owned"
                    @click.stop="emit('toggle-catalog', item)"
                  >
                    {{ item.is_active ? '停用' : '启用' }}
                  </UiButton>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-if="selectedCatalog" class="table-container" style="margin-top: 1.25rem;">
        <table>
          <thead>
            <tr>
              <th>编码</th>
              <th>名称</th>
              <th>排序</th>
              <th>来源</th>
              <th>状态</th>
              <th class="col-actions">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="entries.length === 0">
              <td colspan="6" class="empty-state">该码表暂无项</td>
            </tr>
            <tr v-for="item in entries" :key="`${item.catalog_code}:${item.code}`">
              <td><strong>{{ item.code }}</strong></td>
              <td>{{ item.name }}</td>
              <td>{{ item.rank ?? '—' }}</td>
              <td>{{ sourceLabel(item.source) }}</td>
              <td>
                <UiPill :tone="item.is_active ? 'ok' : 'mute'">
                  {{ item.is_active ? '启用中' : '已停用' }}
                </UiPill>
              </td>
              <td>
                <div class="row-actions">
                  <UiButton v-if="canManage" @click="emit('open-entry', item)">编辑</UiButton>
                  <UiButton v-if="canManage" @click="emit('toggle-entry', item)">
                    {{ item.is_active ? '停用' : '启用' }}
                  </UiButton>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <UiModal :open="catalogModalShow" :title="editingCatalog ? '编辑码表' : '新建码表'" @close="emit('close')">
      <div class="form-grid">
        <label>
          编码
          <input
            :value="catalogForm.code"
            :disabled="Boolean(editingCatalog)"
            @input="patchCatalog('code', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label>
          名称
          <input
            :value="catalogForm.name"
            @input="patchCatalog('name', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label class="span-2">
          说明
          <input
            :value="catalogForm.description"
            @input="patchCatalog('description', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label>
          <input
            type="checkbox"
            :checked="catalogForm.is_open"
            @change="patchCatalog('is_open', ($event.target as HTMLInputElement).checked)"
          >
          开放（允许导入自动加项）
        </label>
        <label>
          <input
            type="checkbox"
            :checked="catalogForm.is_ordered"
            @change="patchCatalog('is_ordered', ($event.target as HTMLInputElement).checked)"
          >
          有序（用 rank 比较）
        </label>
      </div>
      <template #footer>
        <UiButton @click="emit('close')">取消</UiButton>
        <UiButton variant="primary" :disabled="saving" @click="emit('save')">保存</UiButton>
      </template>
    </UiModal>

    <UiModal :open="entryModalShow" :title="editingEntry ? '编辑码表项' : '新建码表项'" @close="emit('close')">
      <div class="form-grid">
        <label>
          编码
          <input
            :value="entryForm.code"
            :disabled="Boolean(editingEntry)"
            @input="patchEntry('code', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label>
          名称
          <input
            :value="entryForm.name"
            @input="patchEntry('name', ($event.target as HTMLInputElement).value)"
          >
        </label>
        <label>
          排序
          <input
            :value="entryForm.rank"
            placeholder="可空"
            @input="patchEntry('rank', ($event.target as HTMLInputElement).value)"
          >
        </label>
      </div>
      <template #footer>
        <UiButton @click="emit('close')">取消</UiButton>
        <UiButton variant="primary" :disabled="saving" @click="emit('save')">保存</UiButton>
      </template>
    </UiModal>
  </section>
</template>
