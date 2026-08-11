import { describe, expect, it, vi } from 'vitest';
import { patchFlightBatchCells } from './useFlightCrud';

function mockResponse(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body),
    json: async () => body,
  } as Response;
}

describe('patchFlightBatchCells', () => {
  it('sends targets + client_action_id (not items) and unwraps data', async () => {
    const authFetch = vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body || '{}'));
      expect(body.field).toBe('stand');
      expect(body.value).toBe('A128');
      expect(body.client_action_id).toBe('BATCH01');
      expect(body.targets).toEqual([
        { flight_id: 'f1', expected_version: 3, expected_value: 'A1' },
        { flight_id: 'f2', expected_version: 5, expected_value: null },
      ]);
      expect(body.items).toBeUndefined();
      return mockResponse(200, {
        success: true,
        message: '批量更新成功：2 条',
        data: {
          batch_id: 'BATCH01',
          field: 'stand',
          updated_count: 2,
          results: [
            { flight_id: 'f1', version: 4, value: 'A128' },
            { flight_id: 'f2', version: 6, value: 'A128' },
          ],
        },
      });
    });

    const result = await patchFlightBatchCells(
      {
        field: 'stand',
        value: 'A128',
        client_action_id: 'BATCH01',
        targets: [
          { flight_id: 'f1', expected_version: 3, expected_value: 'A1' },
          { flight_id: 'f2', expected_version: 5, expected_value: null },
        ],
      },
      { apiBase: '/api/v2', authFetch },
    );

    expect(authFetch).toHaveBeenCalledTimes(1);
    expect(String(authFetch.mock.calls[0][0])).toContain('/flights/batch-cells');
    expect(result.updated_count).toBe(2);
    expect(result.batch_id).toBe('BATCH01');
    expect(result.results).toHaveLength(2);
  });

  it('surfaces nested 409 conflict code and message', async () => {
    const authFetch = vi.fn(async () =>
      mockResponse(409, {
        success: false,
        error: {
          code: 'FLIGHT_BATCH_CONFLICT',
          message: '2 个航班冲突，未写入',
          type: 'conflict_error',
        },
      }),
    );

    await expect(
      patchFlightBatchCells(
        {
          field: 'flight_remarks',
          value: 'note',
          targets: [{ flight_id: 'f1', expected_version: 1, expected_value: 'old note' }],
        },
        { apiBase: '/api/v2', authFetch },
      ),
    ).rejects.toThrow(/FLIGHT_BATCH_CONFLICT/);
  });

  it('rejects empty field or empty targets before network', async () => {
    const authFetch = vi.fn();
    await expect(
      patchFlightBatchCells(
        { field: '', value: 'x', targets: [{ flight_id: 'f1', expected_value: null }] },
        { apiBase: '/api/v2', authFetch },
      ),
    ).rejects.toThrow(/字段缺失/);
    await expect(
      patchFlightBatchCells(
        { field: 'stand', value: 'A1', targets: [] },
        { apiBase: '/api/v2', authFetch },
      ),
    ).rejects.toThrow(/目标航班为空/);
    expect(authFetch).not.toHaveBeenCalled();
  });

  it('rejects a target without expected_value before network', async () => {
    const authFetch = vi.fn();

    await expect(
      patchFlightBatchCells(
        {
          field: 'stand',
          value: 'A1',
          targets: [{ flight_id: 'f1', expected_version: 1, expected_value: undefined }],
        },
        { apiBase: '/api/v2', authFetch },
      ),
    ).rejects.toThrow(/expected_value/);
    expect(authFetch).not.toHaveBeenCalled();
  });
});
