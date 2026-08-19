<template>
  <div
    class="ai-react-entry-shell"
    :class="[`ai-react-entry-shell--${surface}`, `ai-react-entry-shell--status-${status}`]"
  >
    <div
      :id="hostId"
      ref="hostEl"
      class="ai-react-entry-shell__host"
      :data-ai-entry="entryName"
      :data-ai-features="featuresAttr || undefined"
    />

    <div
      v-if="status === 'loading'"
      class="ai-react-entry-shell__loading"
      role="status"
      aria-live="polite"
    >
      <span class="ai-react-entry-shell__spinner" aria-hidden="true" />
      <span>{{ loadingLabel }}</span>
    </div>

    <div
      v-else-if="status === 'error'"
      class="ai-react-entry-shell__error"
      role="alert"
      aria-live="assertive"
    >
      <p class="ai-react-entry-shell__error-eyebrow">
        AI 静态资源未构建
      </p>
      <h2 class="ai-react-entry-shell__error-title">
        {{ label }} 暂时无法加载
      </h2>
      <p class="ai-react-entry-shell__error-message">
        {{ errorMessage }}
      </p>
      <p class="ai-react-entry-shell__error-hint">
        操作建议：进入 <code>frontend/ai-react</code> 目录执行
        <code>npm run build</code>，然后点击下方按钮重试。
      </p>
      <button
        type="button"
        class="ai-react-entry-shell__retry"
        :disabled="retrying"
        @click="onRetry"
      >
        {{ retrying ? '重试中…' : '重试加载' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue';
import {
  useAiReactEntry,
  type AiReactEntrySurface,
} from '@/composables/useAiReactEntry';
import type { AiEntryName } from '@/legacy/aiEntryLoader';
import { installAiAuthBridge } from '@/legacy/installAiAuthBridge';

const props = withDefaults(
  defineProps<{
    entryName: AiEntryName;
    surface?: AiReactEntrySurface;
    loadingLabel?: string;
    /**
     * Playground capabilities (Task C5) to enable in the React entry, e.g.
     * ['plan-board', 'subagent-tree', 'run-resume', 'compression-notice'].
     * Forwarded as the host's `data-ai-features` attribute; when omitted the
     * React side keeps every feature enabled (legacy-host behavior).
     */
    entryFeatures?: string[];
  }>(),
  {
    surface: 'page',
    loadingLabel: '加载 AI 模块中…',
    entryFeatures: undefined,
  },
);

const featuresAttr = computed(() => (props.entryFeatures || []).join(','));

const AI_ENTRY_HOST_IDS: Record<AiEntryName, string> = {
  ai_monitor: 'ai-react-root',
  nl_query: 'ai-react-root',
  dispatch_board_ai: 'dispatch-ai-root',
};

// The Vue application owns authentication. Install its bridge synchronously
// during setup so no retained React module can execute before the boundary exists.
installAiAuthBridge();

const { hostRef, status, errorMessage, retry, label, surface } = useAiReactEntry(
  props.entryName,
  { surface: props.surface },
);

const hostEl = ref<HTMLElement | null>(null);
const hostId = computed(() => AI_ENTRY_HOST_IDS[props.entryName]);

watchEffect(() => {
  hostRef.value = hostEl.value;
});

const retrying = ref(false);
async function onRetry() {
  retrying.value = true;
  try {
    await retry();
  } finally {
    retrying.value = false;
  }
}
</script>

<style scoped>
.ai-react-entry-shell {
  position: relative;
  min-height: 240px;
  width: 100%;
}

.ai-react-entry-shell--page {
  min-height: calc(100vh - 80px);
}

.ai-react-entry-shell__host {
  min-height: inherit;
}

.ai-react-entry-shell__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 48px 16px;
  color: var(--fms-color-text-secondary, #64748b);
  font-size: 14px;
}

.ai-react-entry-shell__spinner {
  width: 18px;
  height: 18px;
  border: 2px solid var(--border-focus);
  border-top-color: var(--system-blue);
  border-radius: 50%;
  animation: ai-react-entry-spin 0.9s linear infinite;
}

@keyframes ai-react-entry-spin {
  to {
    transform: rotate(360deg);
  }
}

.ai-react-entry-shell__error {
  max-width: 640px;
  margin: 64px auto;
  padding: 24px 28px;
  border: 1px solid var(--error-border-subtle);
  border-radius: 14px;
  background: linear-gradient(180deg, var(--glass-bg), rgba(254, 242, 242, 0.92));
  color: var(--fms-color-text-primary, #1D1D1F);
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.08);
}

.ai-react-entry-shell__error-eyebrow {
  margin: 0 0 6px;
  color: var(--ws-danger);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
}

.ai-react-entry-shell__error-title {
  margin: 0 0 8px;
  font-size: 20px;
  font-weight: 700;
}

.ai-react-entry-shell__error-message {
  margin: 0 0 8px;
  color: var(--fms-color-text-secondary, #5f6368);
  line-height: 1.6;
}

.ai-react-entry-shell__error-hint {
  margin: 0 0 16px;
  color: var(--fms-color-text-tertiary, #64748b);
  font-size: 13px;
  line-height: 1.6;
}

.ai-react-entry-shell__error-hint code {
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(15, 23, 42, 0.06);
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
  font-size: 12px;
}

.ai-react-entry-shell__retry {
  appearance: none;
  border: none;
  border-radius: 8px;
  padding: 8px 18px;
  background: var(--system-blue);
  color: var(--text-inverse);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease;
}

.ai-react-entry-shell__retry:hover:not(:disabled) {
  background: var(--system-blue);
}

.ai-react-entry-shell__retry:disabled {
  background: #94a3b8;
  cursor: not-allowed;
}

.ai-react-entry-shell--widget,
.ai-react-entry-shell--drawer,
.ai-react-entry-shell--modal {
  min-height: 180px;
}

.ai-react-entry-shell--widget .ai-react-entry-shell__error,
.ai-react-entry-shell--drawer .ai-react-entry-shell__error,
.ai-react-entry-shell--modal .ai-react-entry-shell__error {
  margin: 16px;
  padding: 16px;
}
</style>
