import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest';

// Mock localStorage and location before importing useAuth
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
  Object.defineProperty(globalThis, 'location', {
    value: { href: '' },
    writable: true,
  });
});

// Mock useAuth before importing useApi
const mockFetch = vi.fn();

vi.mock('./useAuth', () => ({
  useAuth: () => ({
    fetch: mockFetch,
    apiBase: { value: '' },
  }),
}));

import { useApi } from './useApi';

describe('useApi interceptors', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockFetch.mockResolvedValue(new Response(null, { status: 200 }));
  });

  it('calls request interceptor before request', async () => {
    const api = useApi();
    const interceptor = vi.fn((req: Request) => req);
    api.addRequestInterceptor(interceptor);

    await api.raw('http://localhost/test');

    expect(interceptor).toHaveBeenCalledTimes(1);
  });

  it('calls response interceptor after response', async () => {
    const api = useApi();
    const interceptor = vi.fn((res: Response) => res);
    api.addResponseInterceptor(interceptor);

    await api.raw('http://localhost/test');

    expect(interceptor).toHaveBeenCalledTimes(1);
  });

  it('response interceptor handles 401 redirect', async () => {
    const api = useApi();

    mockFetch.mockResolvedValue(new Response(null, { status: 401 }));
    api.addResponseInterceptor((res: Response) => {
      if (res.status === 401) {
        window.location.href = '/login';
      }
      return res;
    });

    await api.raw('http://localhost/test');

    expect(window.location.href).toBe('/login');
  });

  it('addRequestInterceptor returns removal function', async () => {
    const api = useApi();
    const interceptor = vi.fn((req: Request) => req);
    const remove = api.addRequestInterceptor(interceptor);

    remove();
    await api.raw('http://localhost/test');

    expect(interceptor).not.toHaveBeenCalled();
  });

  it('addResponseInterceptor returns removal function', async () => {
    const api = useApi();
    const interceptor = vi.fn((res: Response) => res);
    const remove = api.addResponseInterceptor(interceptor);

    remove();
    await api.raw('http://localhost/test');

    expect(interceptor).not.toHaveBeenCalled();
  });
});
