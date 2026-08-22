<script setup lang="ts">
import { computed, ref } from 'vue';
import UiButton from '@/components/ui/UiButton.vue';
import UiDrawer from '@/components/ui/UiDrawer.vue';

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
  <!-- 壳层归 UiDrawer；按钮归 UiButton；深色预览块不随主题翻 -->
  <UiDrawer
    :open="open"
    title="规则配置导出"
    :width="540"
    @close="emit('close')"
  >
    <div class="export-body">
      <p class="muted">
        以下 JSON 包含当前选中科室的规则、要求和模板。可下载本地文件或复制到剪贴板用于版本对比。
      </p>

      <div class="actions">
        <UiButton :disabled="exporting" @click="handleCopy">
          {{ exporting ? '处理中…' : '复制 JSON' }}
        </UiButton>
        <UiButton variant="primary" :disabled="exporting" @click="handleDownload">
          {{ exporting ? '处理中…' : '下载 JSON' }}
        </UiButton>
      </div>
      <p v-if="message" class="status" :data-status="status">
        {{ message }}
      </p>

      <h4>预览</h4>
      <pre class="preview">{{ previewText || previewSnapshot }}</pre>
    </div>
  </UiDrawer>
</template>

<style scoped>
.export-body {
  display: flex;
  flex-direction: column;
  gap: var(--s3);
  height: 100%;
}

.muted {
  margin: 0;
  font-size: var(--fs-label);
  color: var(--ink-muted);
}

.actions {
  display: flex;
  gap: var(--s2);
}

.status {
  margin: 0;
  font-size: var(--fs-label);
  padding: var(--s2) var(--s3);
  border-radius: var(--r-cell);
}

.status[data-status='success'] {
  background: var(--ok-soft);
  color: var(--ok);
}

.status[data-status='error'] {
  background: var(--danger-soft);
  color: var(--danger);
}

h4 {
  margin: 0;
  font-size: var(--fs-section);
  font-weight: var(--fw-medium);
  color: var(--ink-subtle);
}

/* 深色代码预览块：固定深底浅字，不随主题翻面 */
.preview {
  background: #0f172a;
  color: #f1f5f9;
  padding: var(--s3);
  border-radius: var(--r-control);
  font-size: var(--fs-label);
  font-family: var(--mono);
  flex: 1;
  overflow: auto;
  margin: 0;
}
</style>
