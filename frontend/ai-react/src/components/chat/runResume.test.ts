import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import { RunResumeBar } from '@/components/chat/RunResumeBar';
import {
  latestRecoverableCheckpoint,
  normalizeCheckpoint,
  toCompressionNotice,
} from '@/components/chat/runResume';

// RunResumeBar lazily calls the REST API from effects; no network in tests.
vi.mock('@/lib/api/aiApi', () => ({
  listAiRunCheckpoints: vi.fn().mockResolvedValue([]),
  resumeAiRun: vi.fn().mockResolvedValue({}),
  cancelAiJob: vi.fn().mockResolvedValue({}),
}));

describe('normalizeCheckpoint / latestRecoverableCheckpoint', () => {
  it('normalizes checkpoint rows from the checkpoints endpoint', () => {
    const item = normalizeCheckpoint({
      checkpoint_id: 'cp-1',
      sequence_no: 3,
      checkpoint_type: 'after_tool',
      created_at: '2026-08-15T10:00:00Z',
    });
    expect(item).toEqual({
      checkpointId: 'cp-1',
      sequenceNo: 3,
      checkpointType: 'after_tool',
      createdAt: '2026-08-15T10:00:00Z',
    });
  });

  it('picks the latest recoverable checkpoint by sequence number', () => {
    const items = [
      { checkpointId: 'cp-input', sequenceNo: 1, checkpointType: 'run_input' },
      { checkpointId: 'cp-before', sequenceNo: 2, checkpointType: 'before_tool' },
      { checkpointId: 'cp-after', sequenceNo: 3, checkpointType: 'after_tool' },
      { checkpointId: 'cp-proposal', sequenceNo: 4, checkpointType: 'before_proposal' },
    ];
    expect(latestRecoverableCheckpoint(items)?.checkpointId).toBe('cp-after');
  });

  it('returns null when no recoverable checkpoint exists', () => {
    expect(
      latestRecoverableCheckpoint([
        { checkpointId: 'cp-input', sequenceNo: 1, checkpointType: 'run_input' },
      ]),
    ).toBeNull();
    expect(latestRecoverableCheckpoint([])).toBeNull();
  });
});

describe('toCompressionNotice', () => {
  it('maps the context.compressed payload', () => {
    const notice = toCompressionNotice({
      run_id: 'run-1',
      strategy: 'summarize',
      before_tokens: 12000,
      after_tokens: 3000,
      latency_ms: 45,
      persisted: true,
    });
    expect(notice.strategy).toBe('summarize');
    expect(notice.beforeTokens).toBe(12000);
    expect(notice.afterTokens).toBe(3000);
    expect(notice.latencyMs).toBe(45);
    expect(notice.at).toBeTruthy();
  });

  it('tolerates missing fields', () => {
    const notice = toCompressionNotice({ run_id: 'run-2' });
    expect(notice.strategy).toBeUndefined();
    expect(notice.beforeTokens).toBeUndefined();
  });
});

describe('RunResumeBar component', () => {
  it('renders resume entry and run id; cancel button only with jobId', () => {
    const html = renderToStaticMarkup(
      createElement(RunResumeBar, { runId: 'run-abc', jobId: 'job-1' }),
    );
    expect(html).toContain('run-resume-bar');
    expect(html).toContain('run-abc');
    expect(html).toContain('恢复运行');
    expect(html).toContain('取消运行');
  });

  it('omits the cancel button when jobId is unknown', () => {
    const html = renderToStaticMarkup(createElement(RunResumeBar, { runId: 'run-abc' }));
    expect(html).toContain('恢复运行');
    expect(html).not.toContain('取消运行');
  });
});
