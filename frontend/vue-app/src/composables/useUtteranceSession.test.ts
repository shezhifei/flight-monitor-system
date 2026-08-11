import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAiBusinessCaseCopilot } from './useAiBusinessCaseCopilot';
import { useUtteranceSession } from './useUtteranceSession';

const copilotMocks = vi.hoisted(() => ({
  get: vi.fn(),
  post: vi.fn(),
}));

vi.mock('./useApi', () => ({
  useApi: () => ({
    get: copilotMocks.get,
    post: copilotMocks.post,
  }),
}));

vi.mock('./useAuth', () => ({
  useAuth: () => ({
    apiBase: { value: 'http://api.test' },
  }),
}));

describe('useUtteranceSession', () => {
  beforeEach(() => {
    copilotMocks.get.mockReset();
    copilotMocks.post.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('waits for the final grace window before requesting a draft', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1200,
      onSegmentReady,
    });

    session.acceptFinal('MU5101 申请加油');

    expect(session.transcript.value).toBe('MU5101 申请加油');
    expect(session.status.value).toBe('collecting');
    expect(onSegmentReady).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1199);
    expect(onSegmentReady).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenCalledWith('MU5101 申请加油');
  });

  it('merges multiple final segments into one draft request', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      onSegmentReady,
    });

    session.acceptFinal('MU5101 申请加油');
    vi.advanceTimersByTime(700);
    session.acceptFinal('预计十分钟后完成');
    vi.advanceTimersByTime(999);

    expect(onSegmentReady).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenCalledWith('MU5101 申请加油\n预计十分钟后完成');
  });

  it('flushes the current session manually without waiting for grace timeout', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      onSegmentReady,
    });

    session.acceptFinal('CA123 需要轮椅服务');
    const flushed = session.flushNow();

    expect(flushed).toBe('CA123 需要轮椅服务');
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenCalledWith('CA123 需要轮椅服务');

    vi.advanceTimersByTime(1000);
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
  });

  it('finalizes partial-only speech before flushing so the last fragment is not lost', async () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      partialFinalGraceMs: 500,
      onSegmentReady,
    });

    session.acceptPartial('MU5101 12A 需要轮椅');
    const pending = session.finalizeAndFlush();

    expect(session.canFlush.value).toBe(true);
    expect(session.status.value).toBe('finalizing');
    expect(session.transcript.value).toBe('');

    vi.advanceTimersByTime(499);
    expect(onSegmentReady).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    await expect(pending).resolves.toBe('MU5101 12A 需要轮椅');

    expect(session.partial.value).toBe('');
    expect(session.transcript.value).toBe('MU5101 12A 需要轮椅');
    expect(session.transcriptNeedsConfirmation.value).toBe(true);
    expect(session.hasUnconfirmedText.value).toBe(true);
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenCalledWith('MU5101 12A 需要轮椅');
  });

  it('can manually finalize partial-only speech without notifying the segment-ready callback', async () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      partialFinalGraceMs: 250,
      onSegmentReady,
    });

    session.acceptPartial('CA123 5C 改座');
    const pending = session.finalizeAndFlush({ notify: false });

    vi.advanceTimersByTime(250);
    await expect(pending).resolves.toBe('CA123 5C 改座');

    expect(session.transcript.value).toBe('CA123 5C 改座');
    expect(session.transcriptNeedsConfirmation.value).toBe(true);
    expect(onSegmentReady).not.toHaveBeenCalled();
  });

  it('confirms a late final for a partial-only flush without duplicating the transcript', async () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      partialFinalGraceMs: 100,
      onSegmentReady,
    });

    session.acceptPartial('MU5101 12A 需要轮椅');
    const pending = session.finalizeAndFlush();
    vi.advanceTimersByTime(100);
    await pending;

    expect(session.transcript.value).toBe('MU5101 12A 需要轮椅');
    expect(session.transcriptNeedsConfirmation.value).toBe(true);
    expect(onSegmentReady).toHaveBeenCalledTimes(1);

    session.acceptFinal('MU5101 12A 需要轮椅');

    expect(session.transcript.value).toBe('MU5101 12A 需要轮椅');
    expect(session.transcriptNeedsConfirmation.value).toBe(false);

    vi.advanceTimersByTime(1000);
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
  });

  it('can flush without notifying the segment-ready callback', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      onSegmentReady,
    });

    session.acceptFinal('CA123 需要轮椅服务');
    const flushed = session.flushNow({ notify: false });

    expect(flushed).toBe('CA123 需要轮椅服务');
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1000);
    expect(onSegmentReady).not.toHaveBeenCalled();
  });

  it('does not notify or collect when final text is empty', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      onSegmentReady,
    });

    session.acceptFinal('   ');

    expect(session.transcript.value).toBe('');
    expect(session.partial.value).toBe('');
    expect(session.status.value).toBe('idle');

    vi.advanceTimersByTime(1000);
    expect(onSegmentReady).not.toHaveBeenCalled();
  });

  it('clears partial transcript and status when the session is cleared', () => {
    const session = useUtteranceSession();

    session.acceptPartial('MU5101');
    expect(session.partial.value).toBe('MU5101');
    expect(session.status.value).toBe('collecting');

    session.clear();

    expect(session.transcript.value).toBe('');
    expect(session.partial.value).toBe('');
    expect(session.status.value).toBe('idle');
    expect(session.canFlush.value).toBe(false);
  });

  it('recovers from error state when a new final segment is accepted', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    const session = useUtteranceSession({
      finalGraceMs: 1000,
      onSegmentReady,
    });

    session.acceptFinal('MU5101 申请加油');
    session.markError();
    expect(session.status.value).toBe('error');

    session.acceptFinal('补充预计十分钟完成');

    expect(session.status.value).toBe('collecting');
    expect(session.transcript.value).toBe('MU5101 申请加油\n补充预计十分钟完成');

    vi.advanceTimersByTime(1000);
    expect(session.status.value).toBe('segment_ready');
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenCalledWith('MU5101 申请加油\n补充预计十分钟完成');
  });

  it('reads the final grace window when scheduling each segment', () => {
    vi.useFakeTimers();
    const onSegmentReady = vi.fn();
    let finalGraceMs = 500;
    const session = useUtteranceSession({
      finalGraceMs: () => finalGraceMs,
      onSegmentReady,
    });

    session.acceptFinal('第一次');
    vi.advanceTimersByTime(500);
    expect(onSegmentReady).toHaveBeenCalledTimes(1);
    expect(onSegmentReady).toHaveBeenLastCalledWith('第一次');

    finalGraceMs = 1200;
    session.acceptFinal('第二次');
    vi.advanceTimersByTime(1199);
    expect(onSegmentReady).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1);
    expect(onSegmentReady).toHaveBeenCalledTimes(2);
    expect(onSegmentReady).toHaveBeenLastCalledWith('第一次\n第二次');
  });
});

describe('useAiBusinessCaseCopilot commit idempotency', () => {
  beforeEach(() => {
    copilotMocks.get.mockReset();
    copilotMocks.post.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('reuses a caller-provided idempotency key across consecutive commit retries', async () => {
    copilotMocks.post.mockResolvedValue({
      ok: true,
      status: 200,
      data: {
        success: true,
        data: {
          batch_id: 'batch-1',
          case_ids: ['case-1'],
          notification_groups: [],
          already_committed: false,
          workflow_dispatch_status: 'not_required',
        },
      },
    });

    const copilot = useAiBusinessCaseCopilot();
    await copilot.commitBatch('batch-1', [], { idempotencyKey: 'batch-1-stable-key' });
    await copilot.commitBatch('batch-1', [], { idempotencyKey: 'batch-1-stable-key' });

    expect(copilotMocks.post).toHaveBeenCalledTimes(2);
    expect(copilotMocks.post.mock.calls[0]?.[1]).toMatchObject({
      idempotency_key: 'batch-1-stable-key',
      actions: [],
    });
    expect(copilotMocks.post.mock.calls[1]?.[1]).toMatchObject({
      idempotency_key: 'batch-1-stable-key',
      actions: [],
    });
  });
});
