import { computed, readonly, ref } from 'vue';

export type LoadingMode = 'fullscreen' | 'skeleton' | 'button';

export interface LoadingOptions {
  mode?: LoadingMode;
  target?: string;
  title?: string;
  message?: string;
  label?: string;
  lines?: number;
  minHeight?: string | number;
}

export interface FullscreenLoadingState {
  visible: boolean;
  count: number;
  title: string;
  message: string;
}

export interface TargetLoadingState {
  key: string;
  mode: Exclude<LoadingMode, 'fullscreen'>;
  count: number;
  message: string;
  label: string;
  lines: number;
  minHeight?: string | number;
}

const DEFAULTS = {
  fullscreen: {
    title: '请稍候',
    message: '正在加载数据',
  },
  skeleton: {
    message: '正在加载内容',
    lines: 4,
  },
  button: {
    label: '处理中...',
  },
} as const;

const fullscreen = ref<FullscreenLoadingState>({
  visible: false,
  count: 0,
  title: DEFAULTS.fullscreen.title,
  message: DEFAULTS.fullscreen.message,
});

const targets = ref<Record<string, TargetLoadingState>>({});

function normalizeMode(mode: LoadingMode | string | undefined): LoadingMode {
  const normalized = String(mode || '').trim().toLowerCase();
  if (normalized === 'button' || normalized === 'skeleton') {
    return normalized;
  }
  return 'fullscreen';
}

function normalizeLineCount(value: number | undefined): number {
  if (!Number.isFinite(value)) {
    return DEFAULTS.skeleton.lines;
  }
  return Math.min(6, Math.max(2, Math.round(Number(value))));
}

function showLoading(options: LoadingOptions = {}): string {
  const mode = normalizeMode(options.mode);
  if (mode === 'fullscreen') {
    fullscreen.value = {
      visible: true,
      count: fullscreen.value.count + 1,
      title: String(options.title ?? '').trim() || DEFAULTS.fullscreen.title,
      message: String(options.message ?? '').trim() || DEFAULTS.fullscreen.message,
    };
    return 'fullscreen';
  }

  const key = String(options.target ?? '').trim();
  if (!key) {
    throw new Error('loading target is required for button and skeleton modes');
  }

  const previous = targets.value[key];
  targets.value = {
    ...targets.value,
    [key]: {
      key,
      mode,
      count: previous && previous.mode === mode ? previous.count + 1 : 1,
      message: String(options.message ?? '').trim() || DEFAULTS.skeleton.message,
      label: String(options.label ?? options.message ?? '').trim() || DEFAULTS.button.label,
      lines: normalizeLineCount(options.lines),
      minHeight: options.minHeight,
    },
  };

  return key;
}

function hideLoading(target?: string | null): boolean {
  if (!target || target === 'fullscreen') {
    if (!fullscreen.value.visible) {
      return false;
    }

    const nextCount = Math.max(0, fullscreen.value.count - 1);
    fullscreen.value = {
      ...fullscreen.value,
      visible: nextCount > 0,
      count: nextCount,
    };
    return true;
  }

  const key = String(target).trim();
  const existing = targets.value[key];
  if (!existing) {
    return false;
  }

  if (existing.count > 1) {
    targets.value = {
      ...targets.value,
      [key]: {
        ...existing,
        count: existing.count - 1,
      },
    };
    return true;
  }

  const nextTargets = { ...targets.value };
  delete nextTargets[key];
  targets.value = nextTargets;
  return true;
}

function resetLoading(): void {
  fullscreen.value = {
    visible: false,
    count: 0,
    title: DEFAULTS.fullscreen.title,
    message: DEFAULTS.fullscreen.message,
  };
  targets.value = {};
}

function isLoading(target?: string): boolean {
  if (!target || target === 'fullscreen') {
    return fullscreen.value.visible;
  }
  return Boolean(targets.value[target]);
}

export function useLoading() {
  return {
    fullscreen: readonly(fullscreen),
    targets: readonly(targets),
    hasLoading: computed(() => fullscreen.value.visible || Object.keys(targets.value).length > 0),
    showLoading,
    show: showLoading,
    hideLoading,
    hide: hideLoading,
    resetLoading,
    isLoading,
  };
}
