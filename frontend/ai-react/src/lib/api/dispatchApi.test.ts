import { beforeEach, describe, expect, it, vi } from 'vitest';
import { applyReplan, loadDispatchConflicts, previewReplan } from './dispatchApi';
import { requestEnvelope } from '@/lib/http/apiClient';

vi.mock('@/lib/http/apiClient', () => ({
  requestEnvelope: vi.fn(),
}));

const requestEnvelopeMock = vi.mocked(requestEnvelope);

beforeEach(() => {
  requestEnvelopeMock.mockReset();
  vi.useRealTimers();
});

describe('dispatchApi', () => {
  it('loads conflicts from the v2 dispatch conflicts endpoint', async () => {
    requestEnvelopeMock.mockResolvedValueOnce({
      conflicts: [{ conflict_id: 'c1' }],
    });

    const rows = await loadDispatchConflicts({
      start_time: '2026-06-08T00:00:00.000Z',
      end_time: '2026-06-08T06:00:00.000Z',
      severity: 'high',
    });

    expect(rows).toEqual([{ conflict_id: 'c1' }]);
    expect(requestEnvelopeMock).toHaveBeenCalledWith(
      '/api/v2/dispatch-orders/conflicts?window_start=2026-06-08T00%3A00%3A00.000Z&window_end=2026-06-08T06%3A00%3A00.000Z&severity=high',
    );
  });

  it('previews replan through v2 without applying changes', async () => {
    requestEnvelopeMock.mockResolvedValueOnce({ suggestions: [{ id: 's1' }] });

    const response = await previewReplan({
      strategy: 'balanced',
      max_suggestions: 20,
      scope: {
        window_start: '2026-06-08T01:00:00.000Z',
        window_end: '2026-06-08T05:00:00.000Z',
      },
    });

    expect(response).toEqual({ suggestions: [{ id: 's1' }] });
    expect(requestEnvelopeMock).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        window_start: '2026-06-08T01:00:00.000Z',
        window_end: '2026-06-08T05:00:00.000Z',
        strategy: 'balanced',
        max_suggestions: 20,
        apply_changes: false,
      }),
    });
  });

  it('applies replan through v2 using the same request shape with apply_changes', async () => {
    requestEnvelopeMock.mockResolvedValueOnce({ applied: true });

    await applyReplan({
      strategy: 'efficiency',
      max_suggestions: 3.8,
      window_start: '2026-06-08T02:00:00.000Z',
      window_end: '2026-06-08T04:00:00.000Z',
    });

    expect(requestEnvelopeMock).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        window_start: '2026-06-08T02:00:00.000Z',
        window_end: '2026-06-08T04:00:00.000Z',
        strategy: 'efficiency',
        max_suggestions: 3,
        apply_changes: true,
      }),
    });
  });

  it('uses the operational conflict window when no explicit replan window is provided', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-06-08T08:00:00.000Z'));
    requestEnvelopeMock.mockResolvedValueOnce({ suggestions: [] });

    await previewReplan({
      strategy: 'balanced',
      max_suggestions: 20,
    });

    expect(requestEnvelopeMock).toHaveBeenCalledWith('/api/v2/dispatch-orders/replan', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        window_start: '2026-06-08T06:00:00.000Z',
        window_end: '2026-06-08T12:00:00.000Z',
        strategy: 'balanced',
        max_suggestions: 20,
        apply_changes: false,
      }),
    });
  });
});
