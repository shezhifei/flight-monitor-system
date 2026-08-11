import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';

const localStorageMock = {
  getItem: vi.fn(() => null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
  length: 0,
  key: vi.fn(() => null),
};

beforeAll(() => {
  Object.defineProperty(globalThis, 'localStorage', { value: localStorageMock, writable: true });
});

const mockFetch = vi.fn();

vi.mock('../useAuth', () => ({
  useAuth: () => ({
    fetch: mockFetch,
    apiBase: { value: '' },
  }),
}));

import { useApi } from '../useApi';

describe('useApi response.ok guard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not throw on 500 with non-JSON body claiming application/json', async () => {
    mockFetch.mockResolvedValue(
      new Response('<html>Server Error</html>', {
        status: 500,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const api = useApi();
    const result = await api.get<{ id: string }>('/test');
    expect(result.ok).toBe(false);
    expect(result.status).toBe(500);
    expect(result.data).toBeNull();
  });

  it('parses JSON on 200', async () => {
    mockFetch.mockResolvedValue(
      new Response('{"id":"abc"}', {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const api = useApi();
    const result = await api.get<{ id: string }>('/test');
    expect(result.ok).toBe(true);
    expect(result.data).toEqual({ id: 'abc' });
  });
});