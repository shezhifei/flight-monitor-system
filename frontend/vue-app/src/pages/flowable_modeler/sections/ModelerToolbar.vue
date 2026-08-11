<script setup lang="ts">
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
      <button class="btn btn-secondary btn-sm" type="button" @click="emit('download')">
        下载
      </button>
      <button
        class="btn btn-secondary btn-sm"
        type="button"
        :disabled="!canGenerateDraft"
        :title="canGenerateDraft ? '' : missingLabel"
        @click="emit('generate-draft')"
      >
        生成草案
      </button>
      <button class="btn btn-secondary btn-sm" type="button" @click="emit('deploy')">
        部署流程
      </button>
      <button class="btn btn-primary btn-sm" type="button" @click="emit('save-config')">
        保存配置
      </button>
    </div>
  </header>
</template>

<style scoped>
/* content-header 基础由 admin-layout 提供；压缩高度适配建模器 */
.diagram-toolbar.content-header {
  min-height: 0;
  padding: 14px 20px;
  flex-shrink: 0;
  align-items: center;
}

.diagram-toolbar .content-title {
  font-size: 18px;
  line-height: 1.25;
}

.diagram-toolbar .content-subtitle {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  max-width: none;
}

.diagram-code {
  font-family: ui-monospace, 'SF Mono', Menlo, monospace;
  font-size: 12px;
}

.diagram-scope-sep {
  opacity: 0.45;
}

.diagram-scope {
  font-size: 12px;
}
</style>
