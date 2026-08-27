<script setup lang="ts">
import { computed } from 'vue';
import type {
  BaggageCarousel,
  CarouselFormData,
  DirectoryModal,
  Gate,
  GateFormData,
  Stand,
  StandFormData,
  Terminal,
  TerminalDirectory,
  TerminalFormData,
} from '@/composables/useTerminalDirectory';
import SvgIcon from '@/components/ui/SvgIcon.vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiModal from '@/components/ui/UiModal.vue';
import UiPill from '@/components/ui/UiPill.vue';
import UiSearch from '@/components/ui/UiSearch.vue';
import UiSelect from '@/components/ui/UiSelect.vue';

const props = defineProps<{
  active: boolean;
  canManage: boolean;
  terminals: Terminal[];
  loading: boolean;
  saving: boolean;
  terminalSearch: string;
  selectedTerminalId: string;
  directory: TerminalDirectory | null;
  contextLoading: boolean;
  attachableStands: Stand[];
  attachStandId: string;
  modal: DirectoryModal;
  terminalForm: TerminalFormData;
  gateForm: GateFormData;
  carouselForm: CarouselFormData;
  standForm: StandFormData;
}>();

const emit = defineEmits<{
  (e: 'update:terminalSearch', value: string): void;
  (e: 'update:attachStandId', value: string): void;
  (e: 'update:terminalForm', value: TerminalFormData): void;
  (e: 'update:gateForm', value: GateFormData): void;
  (e: 'update:carouselForm', value: CarouselFormData): void;
  (e: 'update:standForm', value: StandFormData): void;
  (e: 'select', terminalId: string): void;
  (e: 'open-terminal', item?: Terminal): void;
  (e: 'open-gate', item?: Gate): void;
  (e: 'open-carousel', item?: BaggageCarousel): void;
  (e: 'open-stand', item?: Stand): void;
  (e: 'open-attach-stand'): void;
  (e: 'close'): void;
  (e: 'save'): void;
  (e: 'deactivate-terminal', item: Terminal): void;
  (e: 'detach-stand', item: Stand): void;
  (e: 'detach-gate', item: Gate): void;
  (e: 'detach-carousel', item: BaggageCarousel): void;
  (e: 'deactivate-gate', item: Gate): void;
  (e: 'deactivate-carousel', item: BaggageCarousel): void;
  (e: 'deactivate-stand', item: Stand): void;
  (e: 'reactivate-gate', item: Gate): void;
  (e: 'reactivate-carousel', item: BaggageCarousel): void;
  (e: 'reactivate-stand', item: Stand): void;
}>();

const terminalModalShow = computed(() => props.modal.kind === 'terminal');
const gateModalShow = computed(() => props.modal.kind === 'gate');
const carouselModalShow = computed(() => props.modal.kind === 'carousel');
const standModalShow = computed(() => props.modal.kind === 'stand');
const attachStandModalShow = computed(() => props.modal.kind === 'attach-stand');
const conflictModalShow = computed(() => props.modal.kind === 'conflict');

const editingTerminal = computed(() => (props.modal.kind === 'terminal' ? props.modal.item ?? null : null));
const editingGate = computed(() => (props.modal.kind === 'gate' ? props.modal.item ?? null : null));
const editingCarousel = computed(() => (props.modal.kind === 'carousel' ? props.modal.item ?? null : null));
const editingStand = computed(() => (props.modal.kind === 'stand' ? props.modal.item ?? null : null));
const conflict = computed(() => (props.modal.kind === 'conflict' ? props.modal : null));

const attachStandOptions = computed(() => [
  { value: '', label: '请选择机位' },
  ...props.attachableStands.map((s) => ({
    value: s.id,
    label: s.name ? `${s.code}（${s.name}）` : s.code,
  })),
]);

function patchTerminal<K extends keyof TerminalFormData>(field: K, value: TerminalFormData[K]) {
  emit('update:terminalForm', { ...props.terminalForm, [field]: value });
}
function patchGate<K extends keyof GateFormData>(field: K, value: GateFormData[K]) {
  emit('update:gateForm', { ...props.gateForm, [field]: value });
}
function patchCarousel<K extends keyof CarouselFormData>(field: K, value: CarouselFormData[K]) {
  emit('update:carouselForm', { ...props.carouselForm, [field]: value });
}
function patchStand<K extends keyof StandFormData>(field: K, value: StandFormData[K]) {
  emit('update:standForm', { ...props.standForm, [field]: value });
}

/* 占用明细行：后端 Value 形状不定，通用 key: value 渲染 */
function detailEntries(item: unknown): Array<[string, string]> {
  if (item && typeof item === 'object') {
    return Object.entries(item as Record<string, unknown>).map(([k, v]) => [
      k,
      v === null || v === undefined ? '-' : typeof v === 'object' ? JSON.stringify(v) : String(v),
    ]);
  }
  return [['明细', String(item)]];
}
</script>

<template>
  <section class="section-content" :class="{ active }">
    <div class="content-header">
      <div class="content-heading">
        <div class="content-title">
          空间目录
        </div>
        <div class="content-subtitle">
          航站楼及其机位 / 登机口 / 行李转盘成员关系；新建机位 / 口 / 转盘必须挂楼。
        </div>
      </div>
    </div>
    <div class="content-body">
      <div class="directory-layout">
        <!-- 楼列表 -->
        <div class="terminal-list">
          <div class="section-toolbar">
            <div class="filter-group">
              <UiSearch
                :model-value="terminalSearch"
                label="搜索航站楼"
                placeholder="搜索航站楼..."
                @update:model-value="emit('update:terminalSearch', $event)"
              />
            </div>
            <UiButton
              v-if="canManage"
              variant="primary"
              size="md"
              @click="emit('open-terminal')"
            >
              <SvgIcon src="/frontend/icons/add.svg" :size="14" /> 新建航站楼
            </UiButton>
          </div>
          <div class="table-container">
            <table>
              <thead>
                <tr>
                  <th>代码</th>
                  <th>名称</th>
                  <th>状态</th>
                  <th class="col-actions">
                    操作
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="loading">
                  <td colspan="4" class="empty-state">
                    加载中...
                  </td>
                </tr>
                <tr v-else-if="terminals.length === 0">
                  <td colspan="4" class="empty-state">
                    暂无航站楼
                  </td>
                </tr>
                <tr
                  v-for="t in terminals"
                  :key="t.terminal_id"
                  class="terminal-row"
                  :class="{ selected: selectedTerminalId === t.terminal_id }"
                  @click="emit('select', t.terminal_id)"
                >
                  <td><strong>{{ t.code }}</strong></td>
                  <td>{{ t.name }}</td>
                  <td>
                    <UiPill :tone="t.is_active ? 'ok' : 'mute'">
                      {{ t.is_active ? '启用中' : '已停用' }}
                    </UiPill>
                  </td>
                  <td>
                    <div class="row-actions" @click.stop>
                      <UiButton v-if="canManage" @click="emit('open-terminal', t)">
                        编辑
                      </UiButton>
                      <UiButton
                        v-if="canManage && t.is_active"
                        variant="danger"
                        @click="emit('deactivate-terminal', t)"
                      >
                        停用
                      </UiButton>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- 选中楼的成员上下文 -->
        <div class="terminal-context">
          <template v-if="selectedTerminalId">
            <div v-if="contextLoading" class="empty-state">
              加载成员中...
            </div>
            <template v-else-if="directory">
              <!-- 机位 -->
              <div class="member-block">
                <div class="member-block-head">
                  <h4>机位（{{ directory.stands.length }}）</h4>
                  <div class="row-actions">
                    <UiButton v-if="canManage" size="sm" @click="emit('open-stand')">
                      <SvgIcon src="/frontend/icons/add.svg" :size="12" /> 新建机位
                    </UiButton>
                    <UiButton v-if="canManage" size="sm" variant="tonal" @click="emit('open-attach-stand')">
                      挂载既有
                    </UiButton>
                  </div>
                </div>
                <div class="table-container">
                  <table>
                    <thead>
                      <tr>
                        <th>代码</th>
                        <th>名称</th>
                        <th>区域</th>
                        <th>状态</th>
                        <th class="col-actions">
                          操作
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-if="directory.stands.length === 0">
                        <td colspan="5" class="empty-state">
                          本楼暂无机位
                        </td>
                      </tr>
                      <tr v-for="s in directory.stands" :key="s.id">
                        <td><strong>{{ s.code }}</strong></td>
                        <td>{{ s.name || '-' }}</td>
                        <td>{{ s.area || '-' }}</td>
                        <td>
                          <UiPill :tone="s.is_active === false ? 'mute' : 'ok'">
                            {{ s.is_active === false ? '已停用' : '启用中' }}
                          </UiPill>
                        </td>
                        <td>
                          <div class="row-actions">
                            <UiButton v-if="canManage" @click="emit('open-stand', s)">
                              编辑
                            </UiButton>
                            <UiButton
                              v-if="canManage && s.is_active !== false"
                              variant="danger"
                              @click="emit('deactivate-stand', s)"
                            >
                              停用
                            </UiButton>
                            <UiButton
                              v-if="canManage && s.is_active === false"
                              variant="tonal"
                              @click="emit('reactivate-stand', s)"
                            >
                              启用
                            </UiButton>
                            <UiButton
                              v-if="canManage"
                              variant="quiet"
                              @click="emit('detach-stand', s)"
                            >
                              移出
                            </UiButton>
                          </div>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>

              <!-- 登机口 -->
              <div class="member-block">
                <div class="member-block-head">
                  <h4>登机口（{{ directory.gates.length }}）</h4>
                  <UiButton v-if="canManage" size="sm" @click="emit('open-gate')">
                    <SvgIcon src="/frontend/icons/add.svg" :size="12" /> 新建登机口
                  </UiButton>
                </div>
                <div class="table-container">
                  <table>
                    <thead>
                      <tr>
                        <th>代码</th>
                        <th>名称</th>
                        <th>状态</th>
                        <th class="col-actions">
                          操作
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-if="directory.gates.length === 0">
                        <td colspan="4" class="empty-state">
                          本楼暂无登机口
                        </td>
                      </tr>
                      <tr v-for="g in directory.gates" :key="g.gate_id">
                        <td><strong>{{ g.code }}</strong></td>
                        <td>{{ g.name || '-' }}</td>
                        <td>
                          <UiPill :tone="g.is_active ? 'ok' : 'mute'">
                            {{ g.is_active ? '启用中' : '已停用' }}
                          </UiPill>
                        </td>
                        <td>
                          <div class="row-actions">
                            <UiButton v-if="canManage" @click="emit('open-gate', g)">
                              编辑
                            </UiButton>
                            <UiButton
                              v-if="canManage && g.is_active"
                              variant="danger"
                              @click="emit('deactivate-gate', g)"
                            >
                              停用
                            </UiButton>
                            <UiButton
                              v-if="canManage && !g.is_active"
                              variant="tonal"
                              @click="emit('reactivate-gate', g)"
                            >
                              启用
                            </UiButton>
                            <UiButton
                              v-if="canManage"
                              variant="quiet"
                              @click="emit('detach-gate', g)"
                            >
                              移出
                            </UiButton>
                          </div>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>

              <!-- 行李转盘 -->
              <div class="member-block">
                <div class="member-block-head">
                  <h4>行李转盘（{{ directory.carousels.length }}）</h4>
                  <UiButton v-if="canManage" size="sm" @click="emit('open-carousel')">
                    <SvgIcon src="/frontend/icons/add.svg" :size="12" /> 新建转盘
                  </UiButton>
                </div>
                <div class="table-container">
                  <table>
                    <thead>
                      <tr>
                        <th>代码</th>
                        <th>名称</th>
                        <th>状态</th>
                        <th class="col-actions">
                          操作
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-if="directory.carousels.length === 0">
                        <td colspan="4" class="empty-state">
                          本楼暂无行李转盘
                        </td>
                      </tr>
                      <tr v-for="c in directory.carousels" :key="c.carousel_id">
                        <td><strong>{{ c.code }}</strong></td>
                        <td>{{ c.name || '-' }}</td>
                        <td>
                          <UiPill :tone="c.is_active ? 'ok' : 'mute'">
                            {{ c.is_active ? '启用中' : '已停用' }}
                          </UiPill>
                        </td>
                        <td>
                          <div class="row-actions">
                            <UiButton v-if="canManage" @click="emit('open-carousel', c)">
                              编辑
                            </UiButton>
                            <UiButton
                              v-if="canManage && c.is_active"
                              variant="danger"
                              @click="emit('deactivate-carousel', c)"
                            >
                              停用
                            </UiButton>
                            <UiButton
                              v-if="canManage && !c.is_active"
                              variant="tonal"
                              @click="emit('reactivate-carousel', c)"
                            >
                              启用
                            </UiButton>
                            <UiButton
                              v-if="canManage"
                              variant="quiet"
                              @click="emit('detach-carousel', c)"
                            >
                              移出
                            </UiButton>
                          </div>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </template>
          </template>
          <div v-else class="empty-state context-placeholder">
            点击左侧航站楼查看并维护其机位 / 登机口 / 行李转盘。
          </div>
        </div>
      </div>
    </div>

    <!-- 航站楼新建/编辑 -->
    <UiModal
      :open="terminalModalShow"
      :title="editingTerminal ? '编辑航站楼' : '新建航站楼'"
      :width="420"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="term-code">代码 <span class="required">*</span></label>
        <input
          id="term-code"
          type="text"
          :value="terminalForm.code"
          placeholder="例如：T1"
          :disabled="Boolean(editingTerminal)"
          @input="patchTerminal('code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="term-name">名称 <span class="required">*</span></label>
        <input
          id="term-name"
          type="text"
          :value="terminalForm.name"
          placeholder="例如：一号航站楼"
          @input="patchTerminal('name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!terminalForm.code.trim() || !terminalForm.name.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <!-- 登机口新建/编辑（新建即挂当前楼） -->
    <UiModal
      :open="gateModalShow"
      :title="editingGate ? '编辑登机口' : `新建登机口（${directory?.terminal.code ?? ''}）`"
      :width="420"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="gate-code">代码 <span class="required">*</span></label>
        <input
          id="gate-code"
          type="text"
          :value="gateForm.code"
          placeholder="例如：G-A01"
          :disabled="Boolean(editingGate)"
          @input="patchGate('code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="gate-name">名称</label>
        <input
          id="gate-name"
          type="text"
          :value="gateForm.name"
          placeholder="可选"
          @input="patchGate('name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!gateForm.code.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <!-- 转盘新建/编辑 -->
    <UiModal
      :open="carouselModalShow"
      :title="editingCarousel ? '编辑行李转盘' : `新建行李转盘（${directory?.terminal.code ?? ''}）`"
      :width="420"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="car-code">代码 <span class="required">*</span></label>
        <input
          id="car-code"
          type="text"
          :value="carouselForm.code"
          placeholder="例如：B1"
          :disabled="Boolean(editingCarousel)"
          @input="patchCarousel('code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="car-name">名称</label>
        <input
          id="car-name"
          type="text"
          :value="carouselForm.name"
          placeholder="可选"
          @input="patchCarousel('name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!carouselForm.code.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <!-- 机位新建/编辑（新建即挂当前楼） -->
    <UiModal
      :open="standModalShow"
      :title="editingStand ? '编辑机位' : `新建机位（${directory?.terminal.code ?? ''}）`"
      :width="420"
      @close="emit('close')"
    >
      <div class="form-group">
        <label for="stand-code">代码 <span class="required">*</span></label>
        <input
          id="stand-code"
          type="text"
          :value="standForm.code"
          placeholder="例如：A12"
          :disabled="Boolean(editingStand)"
          @input="patchStand('code', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="stand-name">名称</label>
        <input
          id="stand-name"
          type="text"
          :value="standForm.name"
          placeholder="可选"
          @input="patchStand('name', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="stand-area">区域</label>
        <input
          id="stand-area"
          type="text"
          :value="standForm.area"
          placeholder="例如：近机位"
          @input="patchStand('area', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="stand-type">类型</label>
        <input
          id="stand-type"
          type="text"
          :value="standForm.stand_type"
          placeholder="可选，如接触式"
          @input="patchStand('stand_type', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <div class="form-group">
        <label for="stand-size">机型等级</label>
        <input
          id="stand-size"
          type="text"
          :value="standForm.size_category"
          placeholder="可选，如 C / E"
          @input="patchStand('size_category', ($event.target as HTMLInputElement).value)"
        >
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!standForm.code.trim() || saving"
          @click="emit('save')"
        >
          {{ saving ? '保存中...' : '保存' }}
        </UiButton>
      </template>
    </UiModal>

    <!-- 挂载既有机位 -->
    <UiModal
      :open="attachStandModalShow"
      :title="`挂载机位到 ${directory?.terminal.code ?? ''}`"
      :width="420"
      @close="emit('close')"
    >
      <div class="form-group">
        <UiSelect
          :model-value="attachStandId"
          :options="attachStandOptions"
          label="选择机位"
          min-width="100%"
          @update:model-value="emit('update:attachStandId', $event)"
        />
        <p class="form-hint">
          只列出尚未挂到本楼的启用机位。新机位请用「新建机位」，会同时建档并挂到当前楼。
        </p>
      </div>
      <template #footer>
        <UiButton size="md" @click="emit('close')">
          取消
        </UiButton>
        <UiButton
          size="md"
          variant="primary"
          :disabled="!attachStandId || saving"
          @click="emit('save')"
        >
          {{ saving ? '挂载中...' : '挂载' }}
        </UiButton>
      </template>
    </UiModal>

    <!-- 409 占用明细 -->
    <UiModal
      :open="conflictModalShow"
      :title="conflict?.title ?? '操作被占用阻止'"
      :width="560"
      @close="emit('close')"
    >
      <p class="conflict-message">
        {{ conflict?.message }}
      </p>
      <div v-if="conflict && conflict.details.length > 0" class="table-container">
        <table>
          <tbody>
            <tr v-for="(item, idx) in conflict.details" :key="idx">
              <td>
                <div v-for="([k, v]) in detailEntries(item)" :key="k" class="conflict-entry">
                  <span class="conflict-key">{{ k }}</span>
                  <span>{{ v }}</span>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-else class="form-hint">
        后端未返回占用明细。
      </p>
      <template #footer>
        <UiButton size="md" variant="primary" @click="emit('close')">
          知道了
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

.directory-layout {
  display: grid;
  grid-template-columns: minmax(320px, 5fr) 7fr;
  gap: var(--s4);
  align-items: start;
}

.terminal-row {
  cursor: pointer;
}

.terminal-row.selected {
  background: var(--face-hover, rgba(0, 0, 0, 0.04));
}

.member-block {
  margin-bottom: var(--s4);
}

.member-block-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--s2);
}

.member-block-head h4 {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
  color: var(--ink);
}

.context-placeholder {
  padding: var(--s6) var(--s4);
  color: var(--ink-muted);
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

.form-group input[type="text"] {
  width: 100%;
  height: var(--h-md);
  padding: 0 var(--s3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-control);
  font-size: var(--fs-body);
  background: var(--face-page);
  color: var(--ink);
  box-sizing: border-box;
  font-family: inherit;
}

.form-group input:focus-visible {
  outline: 2px solid var(--act);
  outline-offset: 1px;
}

.form-hint {
  margin: var(--s1) 0 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.conflict-message {
  margin: 0 0 var(--s3);
  color: var(--danger);
  font-weight: var(--fw-medium);
}

.conflict-entry {
  display: flex;
  gap: var(--s2);
  font-size: var(--fs-label);
}

.conflict-key {
  color: var(--ink-muted);
  min-width: 96px;
}
</style>
