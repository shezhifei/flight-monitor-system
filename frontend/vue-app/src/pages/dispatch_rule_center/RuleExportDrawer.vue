<script setup lang="ts">
import { computed, ref } from 'vue';

const props = defineProps<{
  open: boolean;
  exporting: boolean;
  buildSnapshot: () => unknown;
  onCopy: () => Promise<string>;
  onDownload: () => Promise<string>;
}>();

const emit = defineEmits<{ (e: 'close'): void }>();

const status = ref<'idle' | 'success' | 'error'>('idle');
const message = ref('');
const previewText = ref('');

const previewSnapshot = computed(() => {
  try {
    return JSON.stringify(props.buildSnapshot(), null, 2);
  } catch (e) {
    return e instanceof Error ? `// ${e.message}` : '// 无法构建预览';
  }
});

async function handleCopy(): Promise<void> {
  try {
    const text = await props.onCopy();
    previewText.value = text;
    status.value = 'success';
    message.value = '已复制到剪贴板';
  } catch (e) {
    status.value = 'error';
    message.value = e instanceof Error ? e.message : '复制失败';
  }
}

async function handleDownload(): Promise<void> {
  try {
    const text = await props.onDownload();
    previewText.value = text;
    status.value = 'success';
    message.value = '已下载 JSON 文件';
  } catch (e) {
    status.value = 'error';
    message.value = e instanceof Error ? e.message : '下载失败';
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="drawer-backdrop" @click.self="emit('close')">
      <aside class="drawer" role="dialog" aria-label="规则配置导出">
        <header class="drawer-head">
          <h3>规则配置导出</h3>
          <button
            type="button"
            class="close"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </header>

        <p class="muted">
          以下 JSON 包含当前选中科室的规则、要求和模板。可下载本地文件或复制到剪贴板用于版本对比。
        </p>

        <div class="actions">
          <button
            type="button"
            class="btn"
            :disabled="exporting"
            @click="handleCopy"
          >
            {{ exporting ? '处理中…' : '复制 JSON' }}
          </button>
          <button
            type="button"
            class="btn primary"
            :disabled="exporting"
            @click="handleDownload"
          >
            {{ exporting ? '处理中…' : '下载 JSON' }}
          </button>
        </div>
        <p v-if="message" class="status" :data-status="status">
          {{ message }}
        </p>

        <h4>预览</h4>
        <pre class="preview">{{ previewText || previewSnapshot }}</pre>
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.drawer-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.45);
  display: flex;
  justify-content: flex-end;
  z-index: 1000;
}
.drawer {
  width: min(540px, 100vw);
  background: var(--bg-card);
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px;
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.12);
  overflow-y: auto;
}
.drawer-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.drawer-head h3 { margin: 0; font-size: 16px; }
.close {
  background: transparent;
  border: none;
  font-size: 24px;
  line-height: 1;
  cursor: pointer;
  color: var(--text-secondary);
}
.muted { font-size: 12px; color: var(--text-tertiary); margin: 0; }
.actions {
  display: flex;
  gap: 8px;
}
.btn {
  border: 1px solid var(--border-light);
  border-radius: 8px;
  background: var(--bg-card);
  padding: 8px 16px;
  cursor: pointer;
  font-size: 13px;
}
.btn.primary { background: var(--system-blue); color: var(--text-inverse); border-color: var(--system-blue); }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.status {
  margin: 0;
  font-size: 12px;
  padding: 6px 8px;
  border-radius: 6px;
}
.status[data-status='success'] {
  background: var(--success-bg-subtle);
  color: var(--status-text-departed);
}
.status[data-status='error'] {
  background: var(--error-bg-subtle);
  color: var(--ws-danger);
}
h4 { margin: 0; font-size: 13px; color: var(--text-secondary); }
.preview {
  background: #0f172a;
  color: #f1f5f9;
  padding: 12px;
  border-radius: 8px;
  font-size: 11px;
  flex: 1;
  overflow: auto;
  margin: 0;
}
</style>
