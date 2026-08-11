import { computed, readonly, ref } from 'vue';

export type UtteranceSessionStatus =
  | 'idle'
  | 'collecting'
  | 'finalizing'
  | 'segment_ready'
  | 'drafting'
  | 'needs_confirmation'
  | 'error';

export type UtteranceSegmentConfidence = 'final' | 'partial';

interface UtteranceSegment {
  text: string;
  confidence: UtteranceSegmentConfidence;
}

export interface UtteranceSessionOptions {
  finalGraceMs?: number | (() => number | null | undefined);
  partialFinalGraceMs?: number | (() => number | null | undefined);
  onSegmentReady?: (transcript: string) => void;
}

export interface FlushUtteranceOptions {
  notify?: boolean;
}

export const DEFAULT_UTTERANCE_FINAL_GRACE_MS = 1600;
export const DEFAULT_UTTERANCE_PARTIAL_FINAL_GRACE_MS = 600;

export function useUtteranceSession(options: UtteranceSessionOptions = {}) {
  const segments = ref<UtteranceSegment[]>([]);
  const partial = ref('');
  const status = ref<UtteranceSessionStatus>('idle');
  let readyTimer: ReturnType<typeof setTimeout> | null = null;
  let finalizingTimer: ReturnType<typeof setTimeout> | null = null;
  let finalizingPromise: Promise<string> | null = null;
  let resolveFinalizing: ((transcript: string) => void) | null = null;
  let finalizingNotify = true;
  let readyTranscript = '';

  const transcript = computed(() => segments.value.map((segment) => segment.text.trim()).filter(Boolean).join('\n'));
  const hasPendingPartial = computed(() => partial.value.trim().length > 0);
  const transcriptNeedsConfirmation = computed(() => segments.value.some((segment) => segment.confidence === 'partial'));
  const hasUnconfirmedText = computed(() => hasPendingPartial.value || transcriptNeedsConfirmation.value);
  const canFlush = computed(() => transcript.value.trim().length > 0 || hasPendingPartial.value);

  function clearReadyTimer(): void {
    if (readyTimer) {
      clearTimeout(readyTimer);
      readyTimer = null;
    }
  }

  function clearFinalizingTimer(): void {
    if (finalizingTimer) {
      clearTimeout(finalizingTimer);
      finalizingTimer = null;
    }
  }

  function resolveGraceMs(value: number | (() => number | null | undefined) | null | undefined, fallback: number): number {
    const configured = typeof value === 'function' ? value() : value;
    return typeof configured === 'number' && Number.isFinite(configured) && configured >= 0
      ? configured
      : fallback;
  }

  function resolveFinalGraceMs(): number {
    return resolveGraceMs(options.finalGraceMs, DEFAULT_UTTERANCE_FINAL_GRACE_MS);
  }

  function resolvePartialFinalGraceMs(): number {
    return resolveGraceMs(options.partialFinalGraceMs, DEFAULT_UTTERANCE_PARTIAL_FINAL_GRACE_MS);
  }

  function replaceSegmentsFromText(text: string, confidence: UtteranceSegmentConfidence = 'final'): void {
    const normalized = text.trim();
    segments.value = normalized ? [{ text: normalized, confidence }] : [];
  }

  function appendSegment(text: string, confidence: UtteranceSegmentConfidence): void {
    const normalized = text.trim();
    if (!normalized) {
      return;
    }
    segments.value = [...segments.value, { text: normalized, confidence }];
  }

  function confirmOrAppendFinalSegment(text: string): boolean {
    const normalized = text.trim();
    if (!normalized) {
      return false;
    }
    const nextSegments = [...segments.value];
    const last = nextSegments[nextSegments.length - 1];
    if (last?.confidence === 'partial') {
      nextSegments[nextSegments.length - 1] = { text: normalized, confidence: 'final' };
      segments.value = nextSegments;
      return last.text.trim() !== normalized;
    }
    if (last?.confidence === 'final' && last.text.trim() === normalized) {
      return false;
    }
    appendSegment(normalized, 'final');
    return true;
  }

  function emitSegmentReady(notify = true): string {
    clearReadyTimer();
    const normalized = transcript.value.trim();
    if (!normalized) {
      return normalized;
    }
    if (normalized === readyTranscript) {
      status.value = 'segment_ready';
      return normalized;
    }
    readyTranscript = normalized;
    status.value = 'segment_ready';
    if (notify) {
      options.onSegmentReady?.(normalized);
    }
    return normalized;
  }

  function scheduleSegmentReady(): void {
    clearReadyTimer();
    readyTimer = setTimeout(() => {
      emitSegmentReady();
    }, resolveFinalGraceMs());
  }

  function commitPendingPartial(): void {
    const normalized = partial.value.trim();
    if (!normalized) {
      return;
    }
    appendSegment(normalized, 'partial');
    partial.value = '';
    readyTranscript = '';
  }

  function finishFinalizing(notify: boolean): string {
    clearFinalizingTimer();
    commitPendingPartial();
    const normalized = emitSegmentReady(notify);
    const resolve = resolveFinalizing;
    finalizingPromise = null;
    resolveFinalizing = null;
    finalizingNotify = true;
    resolve?.(normalized);
    return normalized;
  }

  function acceptPartial(text: string): void {
    partial.value = text.trim();
    if (partial.value || transcript.value.trim()) {
      status.value = 'collecting';
    }
  }

  function acceptFinal(text: string): void {
    const normalized = text.trim();
    partial.value = '';
    if (!normalized) {
      return;
    }
    const wasFinalizing = finalizingPromise !== null;
    const notify = finalizingNotify;
    clearFinalizingTimer();
    const changedTranscript = confirmOrAppendFinalSegment(normalized);
    if (changedTranscript) {
      readyTranscript = '';
    }
    if (wasFinalizing) {
      finishFinalizing(notify);
    } else {
      status.value = 'collecting';
      scheduleSegmentReady();
    }
  }

  function updateTranscript(text: string): void {
    replaceSegmentsFromText(text);
    partial.value = '';
    readyTranscript = '';
    if (!transcript.value.trim()) {
      clearReadyTimer();
      clearFinalizingTimer();
      status.value = 'idle';
    } else if (status.value === 'idle') {
      status.value = 'collecting';
    }
  }

  function flushNow(options: FlushUtteranceOptions = {}): string {
    commitPendingPartial();
    return emitSegmentReady(options.notify !== false);
  }

  function finalizeAndFlush(options: FlushUtteranceOptions = {}): Promise<string> {
    const notify = options.notify !== false;
    if (!partial.value.trim()) {
      return Promise.resolve(emitSegmentReady(notify));
    }
    if (finalizingPromise) {
      finalizingNotify = finalizingNotify && notify;
      return finalizingPromise;
    }
    clearReadyTimer();
    status.value = 'finalizing';
    finalizingNotify = notify;
    finalizingPromise = new Promise((resolve) => {
      resolveFinalizing = resolve;
      finalizingTimer = setTimeout(() => {
        finishFinalizing(finalizingNotify);
      }, resolvePartialFinalGraceMs());
    });
    return finalizingPromise;
  }

  function markDrafting(): void {
    clearReadyTimer();
    status.value = 'drafting';
  }

  function markNeedsConfirmation(): void {
    clearReadyTimer();
    status.value = 'needs_confirmation';
  }

  function markError(): void {
    clearReadyTimer();
    status.value = 'error';
  }

  function clear(): void {
    clearReadyTimer();
    clearFinalizingTimer();
    const resolve = resolveFinalizing;
    segments.value = [];
    partial.value = '';
    readyTranscript = '';
    finalizingPromise = null;
    resolveFinalizing = null;
    finalizingNotify = true;
    status.value = 'idle';
    resolve?.('');
  }

  return {
    transcript: readonly(transcript),
    partial: readonly(partial),
    status: readonly(status),
    hasPendingPartial,
    transcriptNeedsConfirmation,
    hasUnconfirmedText,
    canFlush,
    acceptPartial,
    acceptFinal,
    updateTranscript,
    flushNow,
    finalizeAndFlush,
    markDrafting,
    markNeedsConfirmation,
    markError,
    clear,
  };
}
