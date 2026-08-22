<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { useAiCapabilities } from '@/composables/useAiCapabilities';
import { useAuth } from '@/composables/useAuth';
import { useEntityModalities } from '@/composables/useEntityModalities';
import { useFlowableAiChat } from '@/composables/useFlowableAiChat';
import AiChatModal from '@/components/ai/AiChatModal.vue';
import UiButton from '@/components/ui/UiButton.vue';
import { useFlowableModeler } from './composables/useFlowableModeler';
import ProcessTreePanel from './sections/ProcessTreePanel.vue';
import ModelerToolbar from './sections/ModelerToolbar.vue';
import PropertiesPanel from './sections/PropertiesPanel.vue';
import CreateCaseModal from './sections/CreateCaseModal.vue';

const p = useFlowableModeler();
const auth = useAuth();
const { canUseChat, canGenerateDraft, missingLabel, fetchCapabilities } = useAiCapabilities();
const chat = useFlowableAiChat();
const entityModalities = useEntityModalities();
const FLOWABLE_CHAT_ENTITY_ID = 'default';

watch(p.showAiChat, (open) => {
  if (open && !entityModalities.loaded.value) entityModalities.fetchModalities(FLOWABLE_CHAT_ENTITY_ID);
});

async function loadUserContext(): Promise<void> {
  // 正确端点是 /api/v2/auth/me（没有 /auth/user-context）
  try {
    const res = await p.api.get<{ success?: boolean; data?: Record<string, unknown> }>('/api/v2/auth/me');
    const payload = res.ok ? (res.data?.data ?? (res.data as Record<string, unknown> | undefined)) : null;
    if (payload && typeof payload === 'object') {
      p.applyUserContext(payload as Record<string, unknown>);
      return;
    }
  } catch {
    // fall through to local session
  }
  const user = auth.getUser?.() as Record<string, unknown> | null | undefined;
  if (user) p.applyUserContext(user);
}

onMounted(async () => {
  await loadUserContext();
  await p.fetchCaseTypes();
  // 画布在选中事项类型后才挂载，不在此提前 initModeler
  // AI 能力失败只禁用助手，不阻断建模
  try {
    await fetchCapabilities();
  } catch {
    /* ignore */
  }
});

function setCanvasEl(el: unknown) {
  p.canvasRef.value = (el as HTMLElement | null) ?? null;
}
</script>

<template>
  <!-- 与用户管理/派工规则同一套 admin-container 左右分栏 token -->
  <div class="admin-container flowable-modeler-page">
    <ProcessTreePanel
      :connection-status="p.connectionStatus.value"
      :active-scope-label="p.activeScopeLabel.value"
      :active-tenant-label="p.activeTenantLabel.value"
      :current-scope="p.currentScope.value"
      :has-department-scope="p.hasDepartmentScope.value"
      :search-query="p.searchQuery.value"
      :case-type-load-error="p.caseTypeLoadError.value"
      :filtered-event-list="p.filteredEventList.value"
      :selected-case-id="p.selectedCaseId.value"
      :user-name="p.userName.value"
      :user-role="p.userRole.value"
      :user-avatar="p.userAvatar.value"
      @switch-scope="p.switchScope"
      @update:search-query="p.searchQuery.value = $event"
      @search="p.handleSearch()"
      @select-case-type="p.selectCaseType"
      @create-case="p.openCreateCaseModal"
      @deprecate-case="p.deprecateCaseType"
      @restore-case="p.restoreCaseType"
    />

    <main class="main-content flowable-main">
      <div v-if="!p.hasSelectedDiagram.value" class="empty-state modeler-empty">
        <div class="empty-state-title">
          选择一个业务事项类型
        </div>
        <div class="empty-state-desc">
          从左侧选择事项类型后开始设计流程，并可为表单任务节点配置完整表单。
        </div>
      </div>

      <div v-else class="editor-container">
        <ModelerToolbar
          :diagram-name="p.diagramName.value"
          :diagram-code="p.diagramCode.value"
          :active-scope-label="p.activeScopeLabel.value"
          :active-tenant-label="p.activeTenantLabel.value"
          :can-generate-draft="canGenerateDraft"
          :missing-label="missingLabel"
          @download="p.downloadBpmn"
          @generate-draft="p.generateDraft"
          @deploy="p.deployDiagram"
          @save-config="p.saveConfig"
        />

        <div class="editor-body">
          <div id="bpmn-canvas" :ref="setCanvasEl" class="canvas-host" />
          <PropertiesPanel />
        </div>
      </div>

      <div v-if="p.isLoading.value" class="loading-overlay">
        <div class="spinner" />
      </div>
    </main>

    <UiButton
      class="ai-chat-fab"
      variant="primary"
      size="md"
      :disabled="!canUseChat"
      :title="canUseChat ? 'AI 助手' : missingLabel"
      @click="p.showAiChat.value = true"
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      ><path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" /></svg>
      AI 助手
    </UiButton>

    <AiChatModal
      :show="p.showAiChat.value"
      :messages="chat.messages.value"
      :sending="chat.sending.value"
      :mode="chat.mode.value"
      :tool-items="chat.toolItems.value"
      :pending-actions="chat.pendingActions.value"
      :can-use-chat="canUseChat"
      :missing-label="missingLabel"
      :input-modalities="entityModalities.inputModalities.value"
      :allowed-input-mime-types="entityModalities.allowedInputMimeTypes.value"
      @close="p.showAiChat.value = false"
      @send="(content) => chat.sendMessage(content)"
      @cancel="chat.cancelStream()"
      @update:mode="chat.mode.value = $event"
      @approve="chat.approve($event)"
      @reject="chat.reject($event)"
    />
    <CreateCaseModal
      :open="p.showCreateCaseModal.value"
      :code="p.createCaseCode.value"
      :name="p.createCaseName.value"
      :scope="p.createCaseScope.value"
      :department-label="p.departmentLabel.value"
      :has-department-scope="p.hasDepartmentScope.value"
      :scope-hint="p.createCaseScopeHint.value"
      :error="p.createCaseError.value"
      :submitting="p.createCaseSubmitting.value"
      @close="p.closeCreateCaseModal"
      @submit="p.submitCreateCase"
      @update:code="p.createCaseCode.value = $event"
      @update:name="p.createCaseName.value = $event"
      @update:scope="p.createCaseScope.value = $event"
    />
  </div>
</template>

<style scoped>
/* 壳层走 admin-layout；empty-state / spinner / @keyframes spin 走全局；
   此处仅建模器主区与 FAB 的位置几何 */
.flowable-main {
  position: relative;
  min-height: 0;
}

/* 全局 empty-state 是页级纵向居中，建模器主区要自己充满剩余高度 */
.modeler-empty {
  height: 100%;
}

.editor-container {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  height: 100%;
}

.editor-body {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

/* bpmn-js 要求容器有明确宽高 */
.canvas-host {
  flex: 1;
  min-width: 0;
  min-height: 0;
  height: 100%;
  /* 建模底色：网格点用线色淡混，底走页面 */
  background:
    radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--line) 55%, transparent) 1px, transparent 0) 0 0 / 18px 18px,
    var(--face-page);
  position: relative;
  overflow: hidden;
}

.canvas-host :deep(.bjs-container),
.canvas-host :deep(.djs-container) {
  width: 100% !important;
  height: 100% !important;
}

/* bpmn-js 深色画布反色规则已收进 flowable-modeler.css（token 化同款选择器），
   此处不再重复定义，避免页内第二套色值。 */

.loading-overlay {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--face-work) 72%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 20;
}

/* spinner 几何与全局同名同义，只补尺寸；转圈动画复用全局 @keyframes spin */
.spinner {
  width: 32px;
  height: 32px;
}

/* FAB：库件的壳，只定浮位与圆度；层序用 --z-float */
.ai-chat-fab {
  position: fixed;
  bottom: var(--s5);
  right: var(--s5);
  border-radius: var(--r-pill);
  z-index: var(--z-float);
}
</style>
