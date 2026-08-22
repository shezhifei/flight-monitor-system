<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue';

defineProps<{
  diagramName: string;
  diagramCode: string;
  activeScopeLabel: string;
  activeTenantLabel: string;
  canGenerateDraft: boolean;
  missingLabel: string;
}>();

const emit = defineEmits<{
  (e: 'download'): void;
  (e: 'generate-draft'): void;
  (e: 'deploy'): void;
  (e: 'save-config'): void;
}>();
</script>

<template>
  <!-- 语义对齐 admin content-header：同一套 --admin-* 表面 token -->
  <header class="diagram-toolbar content-header">
    <div class="content-heading diagram-info">
      <div class="diagram-name content-title">
        {{ diagramName }}
      </div>
      <div class="content-subtitle">
        <span class="diagram-code">{{ diagramCode }}</span>
        <span class="diagram-scope-sep" aria-hidden="true">·</span>
        <span class="diagram-scope">{{ activeScopeLabel }} / {{ activeTenantLabel }}</span>
      </div>
    </div>
    <div class="header-actions diagram-actions">
      <UiButton @click="emit('download')">
        下载
      </UiButton>
      <UiButton
        :disabled="!canGenerateDraft"
        :title="canGenerateDraft ? '' : missingLabel"
        @click="emit('generate-draft')"
      >
        生成草案
      </UiButton>
      <UiButton @click="emit('deploy')">
        部署流程
      </UiButton>
      <UiButton variant="primary" @click="emit('save-config')">
        保存配置
      </UiButton>
    </div>
  </header>
</template>

<style scoped>
/* content-header 基础由 admin-layout 提供；按钮归 UiButton；压缩高度适配建模器 */
.diagram-toolbar.content-header {
  min-height: 0;
  padding: var(--s3) var(--s5);
  flex-shrink: 0;
  align-items: center;
}

.diagram-toolbar .content-title {
  font-size: var(--fs-page);
  line-height: 1.25;
}

.diagram-toolbar .content-subtitle {
  display: flex;
  align-items: center;
  gap: var(--s2);
  flex-wrap: wrap;
  max-width: none;
}

.diagram-code {
  font-family: var(--mono);
  font-size: var(--fs-label);
}

.diagram-scope-sep {
  opacity: 0.45;
}

.diagram-scope {
  font-size: var(--fs-label);
}
</style>
