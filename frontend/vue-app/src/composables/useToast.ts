import { readonly, ref } from 'vue';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface ToastOptions {
  title?: string;
  duration?: number;
  persistent?: boolean;
}

export interface ToastItem {
  id: string;
  type: ToastType;
  title: string;
  message: string;
  role: 'status' | 'alert';
  persistent: boolean;
  durationMs: number;
  remainingMs: number;
  timerStartedAt: number | null;
  isClosing: boolean;
  createdAt: number;
}

const DEFAULT_DURATION_MS = 4200;
const EXIT_DURATION_MS = 180;

const TYPE_META: Record<ToastType, { title: string; role: 'status' | 'alert' }> = {
  success: { title: '成功', role: 'status' },
  error: { title: '错误', role: 'alert' },
  warning: { title: '提示', role: 'alert' },
  info: { title: '通知', role: 'status' },
};

const MAX_VISIBLE_TOASTS = 3;
const toasts = ref<ToastItem[]>([]);
const toastQueue = ref<ToastItem[]>([]);
const timers = new Map<string, number>();
let toastSequence = 0;

function processQueue(): void {
  if (toasts.value.length >= MAX_VISIBLE_TOASTS || toastQueue.value.length === 0) {
    return;
  }

  const nextToast = toastQueue.value.shift();
  if (nextToast) {
    toasts.value = [...toasts.value, nextToast];
    startDismissTimer(nextToast.id, nextToast.durationMs);
  }
}

function normalizeType(type: string): ToastType {
  const normalized = String(type || '').trim().toLowerCase();
  if (normalized === 'success' || normalized === 'error' || normalized === 'warning' || normalized === 'info') {
    return normalized;
  }
  return 'info';
}

function normalizeMessage(message: unknown): string {
  return String(message ?? '').trim();
}

function normalizeDuration(duration: number | undefined): number {
  if (!Number.isFinite(duration)) {
    return DEFAULT_DURATION_MS;
  }
  return Math.max(0, Number(duration));
}

function findToast(id: string): ToastItem | undefined {
  return toasts.value.find((toast) => toast.id === id);
}

function clearDismissTimer(id: string): void {
  const timerId = timers.get(id);
  if (timerId) {
    window.clearTimeout(timerId);
    timers.delete(id);
  }
}

function removeToast(id: string): void {
  clearDismissTimer(id);
  toasts.value = toasts.value.filter((toast) => toast.id !== id);
  processQueue();
}

function startDismissTimer(id: string, durationMs: number): void {
  clearDismissTimer(id);
  const toast = findToast(id);
  if (!toast || toast.persistent || durationMs <= 0) {
    return;
  }

  toast.remainingMs = durationMs;
  toast.timerStartedAt = Date.now();
  timers.set(
    id,
    window.setTimeout(() => {
      dismissToast(id);
    }, durationMs),
  );
}

function dismissToast(id: string): void {
  const toast = findToast(id);
  if (!toast || toast.isClosing) {
    return;
  }

  toast.isClosing = true;
  clearDismissTimer(id);
  window.setTimeout(() => {
    removeToast(id);
  }, EXIT_DURATION_MS);
}

function pauseToast(id: string): void {
  const toast = findToast(id);
  if (!toast || toast.persistent || toast.timerStartedAt === null) {
    return;
  }

  const elapsedMs = Math.max(0, Date.now() - toast.timerStartedAt);
  toast.remainingMs = Math.max(0, toast.remainingMs - elapsedMs);
  toast.timerStartedAt = null;
  clearDismissTimer(id);
}

function resumeToast(id: string): void {
  const toast = findToast(id);
  if (!toast || toast.persistent) {
    return;
  }

  if (toast.remainingMs <= 0) {
    dismissToast(id);
    return;
  }

  startDismissTimer(id, toast.remainingMs);
}

function showToast(type: ToastType | string, message: unknown, options: ToastOptions = {}): string | null {
  const normalizedMessage = normalizeMessage(message);
  if (!normalizedMessage) {
    return null;
  }

  const normalizedType = normalizeType(type);
  const meta = TYPE_META[normalizedType];
  const persistent = Boolean(options.persistent);
  const durationMs = persistent ? 0 : normalizeDuration(options.duration);

  toastSequence += 1;
  const id = `toast-${Date.now()}-${toastSequence}`;
  const toast: ToastItem = {
    id,
    type: normalizedType,
    title: String(options.title ?? '').trim() || meta.title,
    message: normalizedMessage,
    role: meta.role,
    persistent,
    durationMs,
    remainingMs: durationMs,
    timerStartedAt: null,
    isClosing: false,
    createdAt: Date.now(),
  };

  if (toasts.value.length < MAX_VISIBLE_TOASTS) {
    toasts.value = [...toasts.value, toast];
    startDismissTimer(id, durationMs);
  } else {
    toastQueue.value.push(toast);
  }
  
  return id;
}

function clearToasts(): void {
  timers.forEach((timerId) => {
    window.clearTimeout(timerId);
  });
  timers.clear();
  toasts.value = [];
  toastQueue.value = [];
}

export function useToast() {
  return {
    toasts: readonly(toasts),
    showToast,
    show: showToast,
    dismissToast,
    pauseToast,
    resumeToast,
    clearToasts,
  };
}
