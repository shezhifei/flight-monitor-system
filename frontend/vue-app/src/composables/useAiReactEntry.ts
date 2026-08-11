import { onMounted, ref, shallowRef, watch, type Ref } from 'vue';
import {
  AI_ENTRY_LABELS,
  EntryNotFoundError,
  ManifestLoadError,
  loadAiReactEntry,
  type AiEntryName,
} from '@/legacy/aiEntryLoader';

export type AiReactEntryStatus = 'idle' | 'loading' | 'loaded' | 'error';

export type AiReactEntrySurface = 'page' | 'widget' | 'drawer' | 'modal';

export type UseAiReactEntryOptions = {
  surface?: AiReactEntrySurface;
  autoLoad?: boolean;
};

export type UseAiReactEntryResult = {
  hostRef: Ref<HTMLElement | null>;
  status: Ref<AiReactEntryStatus>;
  errorMessage: Ref<string>;
  retry: () => Promise<void>;
  label: string;
  surface: AiReactEntrySurface;
};

function describeError(entryName: AiEntryName, error: unknown): string {
  const label = AI_ENTRY_LABELS[entryName] || entryName;
  if (error instanceof EntryNotFoundError) {
    return `${label} 的 React 入口在 manifest.json 中缺失，请重新构建 frontend/ai-react。`;
  }
  if (error instanceof ManifestLoadError) {
    return `AI 静态资源未构建：无法加载 ${label} 所需的 manifest.json（${error.status ?? '网络错误'}）。请先在 frontend/ai-react 运行 npm run build。`;
  }
  if (error instanceof Error) {
    return `${label} 加载失败：${error.message}`;
  }
  return `${label} 加载失败：未知错误。`;
}

export function useAiReactEntry(
  entryName: AiEntryName,
  options: UseAiReactEntryOptions = {},
): UseAiReactEntryResult {
  const { surface = 'page', autoLoad = true } = options;
  const hostRef = shallowRef<HTMLElement | null>(null);
  const status = ref<AiReactEntryStatus>('idle');
  const errorMessage = ref<string>('');
  const mounted = ref(false);

  async function performLoad(force = false): Promise<void> {
    const host = hostRef.value;
    if (!host) {
      status.value = 'error';
      errorMessage.value = '挂载节点尚未就绪。';
      return;
    }
    status.value = 'loading';
    errorMessage.value = '';
    try {
      await loadAiReactEntry(host, entryName, { force });
      status.value = 'loaded';
    } catch (error) {
      status.value = 'error';
      errorMessage.value = describeError(entryName, error);
    }
  }

  async function retry(): Promise<void> {
    await performLoad(true);
  }

  onMounted(() => {
    mounted.value = true;
    if (autoLoad && hostRef.value) {
      void performLoad(false);
    }
  });

  watch(
    hostRef,
    (host) => {
      if (!autoLoad || !mounted.value || !host || status.value !== 'idle') return;
      void performLoad(false);
    },
    { flush: 'post' },
  );

  return {
    hostRef,
    status,
    errorMessage,
    retry,
    label: AI_ENTRY_LABELS[entryName] || entryName,
    surface,
  };
}
